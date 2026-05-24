use std::sync::Arc;

use async_trait::async_trait;
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, OsRng as ChaChaOsRng},
};
use pipa_core::device::{Device, RefreshToken, Scope, SetupCode, StepUpToken};
use pipa_core::error::{CoreError, Result};
use pipa_core::ids::IdGen;
use pipa_core::ports::{AuthStore, PollResult, RefreshTokenIssued};
use pipa_core::time::Clock;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use super::mapping::{device_from_row, pairing_from_row, refresh_from_row, setup_from_row};

/// 90 days in seconds; pairings issue tokens with this TTL by default.
const PAIRING_REFRESH_TTL: i64 = 7_776_000;
const PAIRING_TTL_SECONDS: i64 = 600;
const STEP_UP_TTL_SECONDS: i64 = 300;
const SETUP_CODE_TTL_SECONDS: i64 = 900;

/// Crockford base32 alphabet minus the confusables (I, L, O, U).
const CROCKFORD_SAFE: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

pub struct SqliteAuthStore {
    pool: SqlitePool,
    clock: Arc<dyn Clock>,
    id_gen: Arc<dyn IdGen>,
}

impl SqliteAuthStore {
    pub fn new(pool: SqlitePool, clock: Arc<dyn Clock>, id_gen: Arc<dyn IdGen>) -> Self {
        Self {
            pool,
            clock,
            id_gen,
        }
    }

    fn new_id(&self) -> String {
        self.id_gen.new_ulid().to_string()
    }
}

fn db<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::RepositoryFailure(e.to_string())
}

fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

fn sha256_bytes(s: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hasher.finalize().into()
}

fn random_hex(byte_len: usize) -> String {
    let mut buf = vec![0u8; byte_len];
    OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

fn random_crockford_block(len: usize) -> String {
    let mut buf = vec![0u8; len];
    OsRng.fill_bytes(&mut buf);
    buf.iter()
        .map(|b| {
            let idx = (*b as usize) % CROCKFORD_SAFE.len();
            CROCKFORD_SAFE[idx] as char
        })
        .collect()
}

/// `XXXX-XXXX` with Crockford-safe alphabet.
fn paired_code() -> String {
    format!("{}-{}", random_crockford_block(4), random_crockford_block(4))
}

fn refresh_plaintext() -> String {
    format!("pages_r_{}", random_hex(16))
}

fn encrypt_with_secret(secret: &str, plaintext: &str) -> Result<(String, String)> {
    let key_bytes = sha256_bytes(secret);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let mut nonce_bytes = [0u8; 12];
    use chacha20poly1305::aead::rand_core::RngCore as _;
    ChaChaOsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| CoreError::RepositoryFailure(format!("pairing encrypt: {e}")))?;
    Ok((hex::encode(ct), hex::encode(nonce_bytes)))
}

fn decrypt_with_secret(secret: &str, ct_hex: &str, nonce_hex: &str) -> Result<String> {
    let key_bytes = sha256_bytes(secret);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let ct = hex::decode(ct_hex)
        .map_err(|e| CoreError::RepositoryFailure(format!("pairing ct hex: {e}")))?;
    let nonce_raw = hex::decode(nonce_hex)
        .map_err(|e| CoreError::RepositoryFailure(format!("pairing nonce hex: {e}")))?;
    if nonce_raw.len() != 12 {
        return Err(CoreError::RepositoryFailure("bad nonce length".into()));
    }
    let nonce = Nonce::from_slice(&nonce_raw);
    let pt = cipher
        .decrypt(nonce, ct.as_ref())
        .map_err(|e| CoreError::RepositoryFailure(format!("pairing decrypt: {e}")))?;
    String::from_utf8(pt)
        .map_err(|e| CoreError::RepositoryFailure(format!("pairing utf8: {e}")))
}

#[async_trait]
impl AuthStore for SqliteAuthStore {
    async fn issue_setup_code(&self) -> Result<SetupCode> {
        let code = paired_code();
        let now = self.clock.now();
        let expires_at = now + SETUP_CODE_TTL_SECONDS;
        sqlx::query(
            "INSERT INTO setup_codes (code, created_at, expires_at) VALUES (?, ?, ?)",
        )
        .bind(&code)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(SetupCode {
            code,
            created_at: now,
            expires_at,
            consumed_at: None,
        })
    }

    async fn consume_setup_code(&self, code: &str) -> Result<bool> {
        let now = self.clock.now();
        let row = sqlx::query("SELECT * FROM setup_codes WHERE code = ?")
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(row) = row else { return Ok(false) };
        let setup = setup_from_row(&row)?;
        if setup.consumed_at.is_some() || setup.expires_at < now {
            return Ok(false);
        }
        let res = sqlx::query(
            "UPDATE setup_codes SET consumed_at = ? WHERE code = ? AND consumed_at IS NULL",
        )
        .bind(now)
        .bind(code)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(res.rows_affected() == 1)
    }

    async fn devices_count(&self) -> Result<u64> {
        let n: i64 = sqlx::query("SELECT COUNT(*) AS c FROM devices WHERE revoked_at IS NULL")
            .fetch_one(&self.pool)
            .await
            .map_err(db)?
            .try_get("c")
            .map_err(db)?;
        Ok(n as u64)
    }

    async fn create_device(&self, label: &str, scope: Scope) -> Result<Device> {
        let id = self.new_id();
        let now = self.clock.now();
        sqlx::query(
            "INSERT INTO devices (id, label, scope, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(label)
        .bind(scope.as_str())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(Device {
            id,
            label: label.to_string(),
            scope,
            created_at: now,
            last_seen_at: None,
            revoked_at: None,
        })
    }

    async fn list_devices(&self) -> Result<Vec<Device>> {
        let rows = sqlx::query("SELECT * FROM devices ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(device_from_row).collect()
    }

    async fn revoke_device(&self, id: &str) -> Result<()> {
        let now = self.clock.now();
        let res =
            sqlx::query("UPDATE devices SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
                .bind(now)
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(db)?;
        if res.rows_affected() == 0 {
            return Err(CoreError::NotFound);
        }
        // Tokens cascade-revoke so a revoked device can never refresh again.
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = ? WHERE device_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn touch_device(&self, id: &str) -> Result<()> {
        let now = self.clock.now();
        sqlx::query("UPDATE devices SET last_seen_at = ? WHERE id = ?")
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn issue_refresh(
        &self,
        device_id: &str,
        scope: Scope,
        ttl_seconds: i64,
    ) -> Result<(RefreshToken, String)> {
        let plaintext = refresh_plaintext();
        let hash = sha256_hex(&plaintext);
        let id = self.new_id();
        let now = self.clock.now();
        let expires_at = now + ttl_seconds;
        sqlx::query(
            r#"
            INSERT INTO refresh_tokens
                (id, device_id, token_hash, scope, created_at, expires_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(device_id)
        .bind(&hash)
        .bind(scope.as_str())
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(db)?;

        Ok((
            RefreshToken {
                id,
                device_id: device_id.to_string(),
                token_hash: hash,
                scope,
                created_at: now,
                expires_at,
                rotated_to: None,
                revoked_at: None,
            },
            plaintext,
        ))
    }

    async fn rotate_refresh(&self, plaintext: &str) -> Result<(RefreshToken, String)> {
        let now = self.clock.now();
        let old_hash = sha256_hex(plaintext);

        let mut tx = self.pool.begin().await.map_err(db)?;

        let row = sqlx::query("SELECT * FROM refresh_tokens WHERE token_hash = ?")
            .bind(&old_hash)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        let Some(row) = row else {
            return Err(CoreError::Unauthorized);
        };
        let old = refresh_from_row(&row)?;
        if old.revoked_at.is_some() || old.expires_at < now || old.rotated_to.is_some() {
            return Err(CoreError::Unauthorized);
        }

        let new_plaintext = refresh_plaintext();
        let new_hash = sha256_hex(&new_plaintext);
        let new_id = self.new_id();
        let new_expires = now + (old.expires_at - old.created_at);

        sqlx::query(
            r#"
            INSERT INTO refresh_tokens
                (id, device_id, token_hash, scope, created_at, expires_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&new_id)
        .bind(&old.device_id)
        .bind(&new_hash)
        .bind(old.scope.as_str())
        .bind(now)
        .bind(new_expires)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        sqlx::query(
            "UPDATE refresh_tokens SET rotated_to = ?, revoked_at = ? WHERE id = ?",
        )
        .bind(&new_id)
        .bind(now)
        .bind(&old.id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok((
            RefreshToken {
                id: new_id,
                device_id: old.device_id,
                token_hash: new_hash,
                scope: old.scope,
                created_at: now,
                expires_at: new_expires,
                rotated_to: None,
                revoked_at: None,
            },
            new_plaintext,
        ))
    }

    async fn revoke_refresh(&self, plaintext: &str) -> Result<()> {
        let hash = sha256_hex(plaintext);
        let now = self.clock.now();
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(&hash)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn lookup_refresh(&self, plaintext: &str) -> Result<Option<(RefreshToken, Device)>> {
        let hash = sha256_hex(plaintext);
        let now = self.clock.now();

        let token_row = sqlx::query(
            r#"
            SELECT * FROM refresh_tokens
            WHERE token_hash = ?
              AND revoked_at IS NULL
              AND rotated_to IS NULL
              AND expires_at >= ?
            "#,
        )
        .bind(&hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;

        let Some(token_row) = token_row else { return Ok(None) };
        let token = refresh_from_row(&token_row)?;

        let device_row = sqlx::query("SELECT * FROM devices WHERE id = ?")
            .bind(&token.device_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(device_row) = device_row else { return Ok(None) };
        let device = device_from_row(&device_row)?;
        if device.revoked_at.is_some() {
            return Ok(None);
        }
        Ok(Some((token, device)))
    }

    async fn begin_pairing(&self) -> Result<(String, String)> {
        let code = paired_code();
        let secret = random_hex(16);
        let secret_hash = sha256_hex(&secret);
        let now = self.clock.now();
        let expires_at = now + PAIRING_TTL_SECONDS;
        sqlx::query(
            r#"
            INSERT INTO device_pairings (code, secret_hash, created_at, expires_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&code)
        .bind(&secret_hash)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok((code, secret))
    }

    async fn approve_pairing(
        &self,
        code: &str,
        label: &str,
        scope: Scope,
    ) -> Result<RefreshTokenIssued> {
        let now = self.clock.now();

        // We need the secret to encrypt the refresh plaintext for poll_pairing.
        // The DB only stores the hash, so we ask the row first (the secret
        // itself isn't in the DB — we reuse secret_hash as the key material
        // since the CLI never sees the secret again. Instead, we re-derive the
        // encryption key from `secret_hash`; the CLI will pass the original
        // secret on poll, we'll re-hash it and use the hash as the key).
        let pairing_row = sqlx::query("SELECT * FROM device_pairings WHERE code = ?")
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(pairing_row) = pairing_row else { return Err(CoreError::NotFound) };
        let pairing = pairing_from_row(&pairing_row)?;
        if pairing.expires_at < now {
            return Err(CoreError::InvalidInput("pairing expired".into()));
        }
        if pairing.approved_device_id.is_some() {
            return Err(CoreError::AlreadyExists);
        }

        let device = self.create_device(label, scope).await?;
        let (refresh, plaintext) = self
            .issue_refresh(&device.id, scope, PAIRING_REFRESH_TTL)
            .await?;

        // The pairing.secret_hash IS the SHA-256 of the secret. We use it as
        // the ChaCha20-Poly1305 key directly (32 bytes). On poll the caller
        // sends the secret; we re-hash and compare, then decrypt with the same
        // bytes. This avoids storing the plaintext secret server-side.
        let key_bytes = hex::decode(&pairing.secret_hash)
            .map_err(|e| CoreError::RepositoryFailure(format!("pairing key hex: {e}")))?;
        if key_bytes.len() != 32 {
            return Err(CoreError::RepositoryFailure(
                "pairing secret_hash is not 32 bytes".into(),
            ));
        }
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
        let mut nonce_bytes = [0u8; 12];
        use chacha20poly1305::aead::rand_core::RngCore as _;
        ChaChaOsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| CoreError::RepositoryFailure(format!("pairing encrypt: {e}")))?;
        let enc_hex = hex::encode(ct);
        let nonce_hex = hex::encode(nonce_bytes);

        sqlx::query(
            r#"
            UPDATE device_pairings
            SET approved_device_id = ?, approved_at = ?, refresh_token_id = ?,
                refresh_plaintext_enc = ?, refresh_plaintext_nonce = ?
            WHERE code = ?
            "#,
        )
        .bind(&device.id)
        .bind(now)
        .bind(&refresh.id)
        .bind(&enc_hex)
        .bind(&nonce_hex)
        .bind(code)
        .execute(&self.pool)
        .await
        .map_err(db)?;

        Ok(RefreshTokenIssued {
            device,
            refresh_plaintext: plaintext,
            refresh,
        })
    }

    async fn poll_pairing(&self, code: &str, secret: &str) -> Result<PollResult> {
        let now = self.clock.now();
        let row = sqlx::query("SELECT * FROM device_pairings WHERE code = ?")
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(row) = row else { return Err(CoreError::NotFound) };
        let pairing = pairing_from_row(&row)?;

        let provided_hash = sha256_hex(secret);
        if provided_hash != pairing.secret_hash {
            return Err(CoreError::Unauthorized);
        }

        if pairing.expires_at < now && pairing.approved_at.is_none() {
            return Ok(PollResult::Expired);
        }

        let Some(device_id) = pairing.approved_device_id.as_deref() else {
            return Ok(PollResult::Pending);
        };
        let Some(refresh_id) = pairing.refresh_token_id.as_deref() else {
            return Ok(PollResult::Pending);
        };

        let device_row = sqlx::query("SELECT * FROM devices WHERE id = ?")
            .bind(device_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        let device = device_from_row(&device_row)?;

        let refresh_row = sqlx::query("SELECT * FROM refresh_tokens WHERE id = ?")
            .bind(refresh_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        let refresh = refresh_from_row(&refresh_row)?;

        let enc_hex: Option<String> = row
            .try_get("refresh_plaintext_enc")
            .map_err(db)?;
        let nonce_hex: Option<String> = row
            .try_get("refresh_plaintext_nonce")
            .map_err(db)?;
        let (Some(enc_hex), Some(nonce_hex)) = (enc_hex, nonce_hex) else {
            return Err(CoreError::RepositoryFailure(
                "approved pairing missing encrypted plaintext".into(),
            ));
        };

        let key_bytes = hex::decode(&pairing.secret_hash)
            .map_err(|e| CoreError::RepositoryFailure(format!("pairing key hex: {e}")))?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
        let ct = hex::decode(&enc_hex)
            .map_err(|e| CoreError::RepositoryFailure(format!("pairing ct hex: {e}")))?;
        let nonce_raw = hex::decode(&nonce_hex)
            .map_err(|e| CoreError::RepositoryFailure(format!("pairing nonce hex: {e}")))?;
        if nonce_raw.len() != 12 {
            return Err(CoreError::RepositoryFailure("bad nonce length".into()));
        }
        let nonce = Nonce::from_slice(&nonce_raw);
        let pt = cipher
            .decrypt(nonce, ct.as_ref())
            .map_err(|e| CoreError::RepositoryFailure(format!("pairing decrypt: {e}")))?;
        let plaintext = String::from_utf8(pt)
            .map_err(|e| CoreError::RepositoryFailure(format!("pairing utf8: {e}")))?;

        Ok(PollResult::Approved(RefreshTokenIssued {
            device,
            refresh_plaintext: plaintext,
            refresh,
        }))
    }

    async fn begin_step_up(
        &self,
        device_id: &str,
        operation: &str,
        target: Option<&str>,
        ip_hash: Option<&str>,
    ) -> Result<StepUpToken> {
        let code = paired_code();
        let now = self.clock.now();
        let expires_at = now + STEP_UP_TTL_SECONDS;
        sqlx::query(
            r#"
            INSERT INTO step_up_tokens
                (code, device_id, operation, target, requesting_ip_hash, created_at, expires_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&code)
        .bind(device_id)
        .bind(operation)
        .bind(target)
        .bind(ip_hash)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(StepUpToken {
            code,
            device_id: device_id.to_string(),
            operation: operation.to_string(),
            target: target.map(|s| s.to_string()),
            requesting_ip_hash: ip_hash.map(|s| s.to_string()),
            created_at: now,
            expires_at,
            consumed_at: None,
            confirmed_at: None,
        })
    }

    async fn confirm_step_up(&self, code: &str) -> Result<()> {
        let now = self.clock.now();
        let res = sqlx::query(
            r#"
            UPDATE step_up_tokens
            SET confirmed_at = ?
            WHERE code = ?
              AND consumed_at IS NULL
              AND confirmed_at IS NULL
              AND expires_at >= ?
            "#,
        )
        .bind(now)
        .bind(code)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if res.rows_affected() == 0 {
            return Err(CoreError::Unauthorized);
        }
        Ok(())
    }

    async fn consume_step_up(
        &self,
        code: &str,
        device_id: &str,
        operation: &str,
        target: Option<&str>,
    ) -> Result<bool> {
        let now = self.clock.now();

        let row = sqlx::query("SELECT * FROM step_up_tokens WHERE code = ?")
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(row) = row else { return Ok(false) };
        let token: StepUpToken = super::mapping::step_up_from_row(&row)?;
        if token.consumed_at.is_some()
            || token.confirmed_at.is_none()
            || token.expires_at < now
            || token.device_id != device_id
            || token.operation != operation
            || token.target.as_deref() != target
        {
            return Ok(false);
        }
        let res = sqlx::query(
            "UPDATE step_up_tokens SET consumed_at = ? WHERE code = ? AND consumed_at IS NULL",
        )
        .bind(now)
        .bind(code)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(res.rows_affected() == 1)
    }

    async fn step_up_observe(&self, code: &str) -> Result<&'static str> {
        let now = self.clock.now();
        let row = sqlx::query("SELECT * FROM step_up_tokens WHERE code = ?")
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(row) = row else { return Ok("unknown") };
        let token: StepUpToken = super::mapping::step_up_from_row(&row)?;
        if token.consumed_at.is_some() {
            return Ok("consumed");
        }
        if token.expires_at < now && token.confirmed_at.is_none() {
            return Ok("expired");
        }
        if token.confirmed_at.is_some() {
            return Ok("confirmed");
        }
        Ok("pending")
    }

    async fn step_up_get(&self, code: &str) -> Result<Option<StepUpToken>> {
        let row = sqlx::query("SELECT * FROM step_up_tokens WHERE code = ?")
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(row) = row else { return Ok(None) };
        Ok(Some(super::mapping::step_up_from_row(&row)?))
    }

    async fn get_device(&self, id: &str) -> Result<Option<Device>> {
        let row = sqlx::query("SELECT * FROM devices WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(row) = row else { return Ok(None) };
        Ok(Some(super::mapping::device_from_row(&row)?))
    }
}

// Keep these around — they're useful for non-pairing flows that need symmetric
// encryption with a CLI-provided secret. Unused today; future M3+ work may
// adopt them for richer device-pairing payloads.
#[allow(dead_code)]
fn encrypt_for_pairing(secret: &str, plaintext: &str) -> Result<(String, String)> {
    encrypt_with_secret(secret, plaintext)
}

#[allow(dead_code)]
fn decrypt_for_pairing(secret: &str, ct_hex: &str, nonce_hex: &str) -> Result<String> {
    decrypt_with_secret(secret, ct_hex, nonce_hex)
}
