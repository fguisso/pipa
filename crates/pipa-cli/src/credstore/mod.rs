//! Credential cascade. Each tier implements `CredStore`; `pick_best` walks
//! the tiers top-down and returns the highest-priority available one.
//!
//! Order (matches SECURITY.md §4):
//!   1. OS keychain (`keyring` crate)
//!   2. `pass` (password-store) shell-out
//!   3. `age`-encrypted file        — **stubbed in this milestone; always
//!      reports unavailable so we never silently choose it.**
//!   4. Chmod-600 TOML file at `~/.config/pipa/auth.toml`
//!   5. `PIPA_REFRESH_TOKEN` env var (overrides everything when set)

use anyhow::Result;

pub mod age;
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

/// Try each tier from highest to lowest and return the first one available.
/// `env::EnvStore` short-circuits — if `PIPA_REFRESH_TOKEN` is set, we
/// always pick it (read-only) so CI can override the rest.
pub fn pick_best() -> Box<dyn CredStore> {
    if env::EnvStore::available() {
        return Box::new(env::EnvStore);
    }
    if keyring::KeyringStore::available() {
        return Box::new(keyring::KeyringStore);
    }
    if pass::PassStore::available() {
        return Box::new(pass::PassStore);
    }
    if age::AgeStore::available() {
        return Box::new(age::AgeStore);
    }
    Box::new(file::FileStore::default())
}

/// Used by `whoami` to print which tier holds the credential. Loops the same
/// cascade but stops at the first tier that *actually* has a token for the
/// given server.
pub fn find_holder(server: &str) -> Option<Box<dyn CredStore>> {
    let candidates: Vec<Box<dyn CredStore>> = vec![
        Box::new(env::EnvStore),
        Box::new(keyring::KeyringStore),
        Box::new(pass::PassStore),
        Box::new(age::AgeStore),
        Box::new(file::FileStore::default()),
    ];
    for c in candidates {
        let available = match c.tier_name() {
            "env" => env::EnvStore::available(),
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
