//! Access-token mint/verify and step-up helpers.
//!
//! Token format: `<base64url(payload)>.<base64url(hmac)>`
//!
//! - payload is JSON of `AccessTokenClaims { sub, scope, exp, iat }`.
//! - hmac is HMAC-SHA256(key, "access-token/v1/" || base64url(payload)).
//!
//! We base64url with no padding so the token is URL/header-safe without
//! escaping.

use std::time::{SystemTime, UNIX_EPOCH};

use pipa_adapters::HmacKey;
use pipa_core::device::AccessTokenClaims;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

const ACCESS_DOMAIN: &[u8] = b"access-token/v1/";
#[allow(dead_code)]
const STEP_UP_DOMAIN: &[u8] = b"step-up/v1/";

#[derive(Debug)]
pub enum TokenError {
    Malformed,
    BadSignature,
    Expired,
}

/// Mint an opaque access token. The CLI just echoes this back as
/// `Authorization: Bearer …` — the server is the only party that can verify.
pub fn mint_access_token(
    key: &HmacKey,
    sub: &str,
    scope: &str,
    ttl_sec: i64,
) -> anyhow::Result<(String, AccessTokenClaims)> {
    let iat = unix_now();
    let exp = iat + ttl_sec.max(1);
    let claims = AccessTokenClaims {
        sub: sub.to_string(),
        scope: scope.to_string(),
        exp,
        iat,
    };
    let payload = serde_json::to_vec(&claims)?;
    let payload_b64 = b64url(&payload);
    let mac = sign(key, payload_b64.as_bytes());
    Ok((format!("{payload_b64}.{}", b64url(&mac)), claims))
}

/// Verify an access token and return its claims. Returns `Expired` for
/// well-formed but stale tokens so the caller can give a precise diagnostic.
pub fn verify_access_token(key: &HmacKey, token: &str) -> Result<AccessTokenClaims, TokenError> {
    let (payload_b64, sig_b64) = token.split_once('.').ok_or(TokenError::Malformed)?;
    let provided_sig = b64url_decode(sig_b64).ok_or(TokenError::Malformed)?;
    // Hmac::verify_slice does constant-time comparison internally.
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("any-length HMAC key");
    mac.update(ACCESS_DOMAIN);
    mac.update(payload_b64.as_bytes());
    mac.verify_slice(&provided_sig)
        .map_err(|_| TokenError::BadSignature)?;

    let payload = b64url_decode(payload_b64).ok_or(TokenError::Malformed)?;
    let claims: AccessTokenClaims =
        serde_json::from_slice(&payload).map_err(|_| TokenError::Malformed)?;
    if claims.exp < unix_now() {
        return Err(TokenError::Expired);
    }
    Ok(claims)
}

/// Issue an HMAC-signed cookie value representing a confirmed step-up. The
/// confirm page sets this client-side so a refresh on the browser keeps the
/// confirmation visible. Format mirrors the password-gate cookie:
/// `<code>|<expires>.<sig>`.
///
/// Reserved for the next milestone — the destructive endpoints will look it
/// up when the browser refreshes the confirm page.
#[allow(dead_code)]
pub fn issue_step_up_cookie(key: &HmacKey, code: &str, expires: i64) -> String {
    let body = format!("{code}|{expires}");
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("any-length HMAC key");
    mac.update(STEP_UP_DOMAIN);
    mac.update(body.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("{body}.{sig}")
}

/// Verify and consume a step-up token submitted via the `X-Stepup-Code`
/// header. Returns true when the code was valid for *this* (device,
/// operation, target) tuple. Called by destructive handlers in M4.
pub async fn verify_step_up(
    state: &AppState,
    code: &str,
    device_id: &str,
    operation: &str,
    target: Option<&str>,
) -> anyhow::Result<bool> {
    Ok(state
        .auth
        .consume_step_up(code, device_id, operation, target)
        .await?)
}

fn sign(key: &HmacKey, payload_b64: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("any-length HMAC key");
    mac.update(ACCESS_DOMAIN);
    mac.update(payload_b64);
    mac.finalize().into_bytes().to_vec()
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// --- base64url (no padding) ---------------------------------------------
// RFC 4648 §5. Inline to avoid a new crate dep just for ~50 lines.

const URL_ALPHA: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64url(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() * 4).div_ceil(3));
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(URL_ALPHA[((n >> 18) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 12) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 6) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(URL_ALPHA[((n >> 18) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 12) & 0x3f) as usize] as char);
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(URL_ALPHA[((n >> 18) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 12) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 6) & 0x3f) as usize] as char);
    }
    out
}

fn b64url_decode(input: &str) -> Option<Vec<u8>> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4 + 2);
    let mut buf: u32 = 0;
    let mut bits: u8 = 0;
    for &b in bytes {
        let v = decode_byte(b)?;
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

fn decode_byte(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64url_round_trip() {
        for case in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let enc = b64url(case);
            let dec = b64url_decode(&enc).expect("decode");
            assert_eq!(dec, case, "case {case:?}");
        }
    }

    #[test]
    fn token_round_trip() {
        let key = HmacKey::from_bytes(vec![1u8; 32]);
        let (tok, _) = mint_access_token(&key, "sub", "read:*", 60).unwrap();
        let claims = verify_access_token(&key, &tok).unwrap();
        assert_eq!(claims.sub, "sub");
        assert_eq!(claims.scope, "read:*");
    }
}
