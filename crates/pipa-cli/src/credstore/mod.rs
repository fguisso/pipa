//! Credential cascade. Each tier implements `CredStore`; `pick_best` walks
//! the tiers top-down and returns the highest-priority available one.
//!
//! Order (matches SECURITY.md §4):
//!   1. `PIPA_REFRESH_TOKEN` env var (overrides everything when set)
//!   2. External command vault (`PIPA_SECRET_GET_CMD`/`_SET_CMD` → `op`/`bw`)
//!   3. OS keychain (`keyring` crate)
//!   4. `pass` (password-store) shell-out
//!   5. `age`-encrypted file        — **stubbed in this milestone; always
//!      reports unavailable so we never silently choose it.**
//!   6. Chmod-600 TOML file at `~/.config/pipa/auth.toml`
//!
//! Under `--headless` (see [`set_headless`]) only the two non-interactive,
//! explicit tiers are eligible — the external command vault and
//! `PIPA_REFRESH_TOKEN` — and there is **no fallback**: if neither is
//! configured, credential resolution fails loudly instead of silently reaching
//! for the OS keychain (which can block on a locked keyring) or writing a
//! plaintext-ish file the caller never asked for.

use std::sync::OnceLock;

use anyhow::{Result, bail};

pub mod age;
pub mod cmd;
pub mod env;
pub mod file;
pub mod keyring;
pub mod pass;

pub trait CredStore: Send + Sync {
    /// Short, machine-friendly identifier — used in logs and `whoami`.
    fn tier_name(&self) -> &'static str;
    /// Human-readable label, e.g. `"macOS Keychain"`.
    fn display_name(&self) -> &'static str;
    /// 5-cell security gauge, e.g. `"●●●●● excellent"`.
    fn security_label(&self) -> &'static str;
    fn store(&self, server: &str, refresh: &str) -> Result<()>;
    fn load(&self, server: &str) -> Result<Option<String>>;
    fn delete(&self, server: &str) -> Result<()>;
}

/// Process-global headless switch, set once from the `--headless` CLI flag in
/// `main`. A `OnceLock` avoids threading the flag through every credstore call
/// site (`client_with_access`, `login`, `logout`, `whoami`).
static HEADLESS: OnceLock<bool> = OnceLock::new();

pub fn set_headless(v: bool) {
    let _ = HEADLESS.set(v);
}

pub fn is_headless() -> bool {
    *HEADLESS.get().unwrap_or(&false)
}

/// Try each tier from highest to lowest and return the first available one.
///
/// Normal mode: `PIPA_REFRESH_TOKEN` then the external command vault
/// short-circuit (both read-only overrides), then keyring → pass → age → file.
///
/// Headless mode: only the command vault and `PIPA_REFRESH_TOKEN` are eligible,
/// and an unconfigured environment is a hard error — never a silent fallback.
pub fn pick_best() -> Result<Box<dyn CredStore>> {
    if is_headless() {
        if cmd::CmdStore::available() {
            return Ok(Box::new(cmd::CmdStore));
        }
        if env::EnvStore::available() {
            return Ok(Box::new(env::EnvStore));
        }
        bail!(
            "--headless: no non-interactive credential source is configured.\n  \
             → set PIPA_SECRET_GET_CMD (and optionally PIPA_SECRET_SET_CMD) to an \
             `op`/`bw` command, or\n  \
             → set PIPA_REFRESH_TOKEN to a token.\n  \
             Refusing to fall back to the OS keychain or an on-disk file in headless mode."
        );
    }

    if env::EnvStore::available() {
        return Ok(Box::new(env::EnvStore));
    }
    if cmd::CmdStore::available() {
        return Ok(Box::new(cmd::CmdStore));
    }
    if keyring::KeyringStore::available() {
        return Ok(Box::new(keyring::KeyringStore));
    }
    if pass::PassStore::available() {
        return Ok(Box::new(pass::PassStore));
    }
    if age::AgeStore::available() {
        return Ok(Box::new(age::AgeStore));
    }
    Ok(Box::new(file::FileStore::default()))
}

/// Used by `whoami` to print which tier holds the credential. Loops the same
/// cascade but stops at the first tier that *actually* has a token for the
/// given server. Honours headless mode (only command vault + env are probed).
pub fn find_holder(server: &str) -> Option<Box<dyn CredStore>> {
    let candidates: Vec<Box<dyn CredStore>> = if is_headless() {
        vec![Box::new(cmd::CmdStore), Box::new(env::EnvStore)]
    } else {
        vec![
            Box::new(env::EnvStore),
            Box::new(cmd::CmdStore),
            Box::new(keyring::KeyringStore),
            Box::new(pass::PassStore),
            Box::new(age::AgeStore),
            Box::new(file::FileStore::default()),
        ]
    };
    for c in candidates {
        let available = match c.tier_name() {
            "env" => env::EnvStore::available(),
            "cmd" => cmd::CmdStore::available(),
            "keyring" => keyring::KeyringStore::available(),
            "pass" => pass::PassStore::available(),
            "age" => age::AgeStore::available(),
            "file" => true,
            _ => false,
        };
        if !available {
            continue;
        }
        if matches!(c.load(server), Ok(Some(_))) {
            return Some(c);
        }
    }
    None
}
