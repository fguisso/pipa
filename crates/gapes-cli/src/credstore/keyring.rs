//! Tier 1: OS keychain. Uses the `keyring` crate, which wraps macOS Keychain,
//! Windows Credential Manager, and Linux secret-service / libsecret.
//!
//! Availability check: try to instantiate an `Entry` and call `get_password`.
//! `NoEntry` means we *can* talk to the backend, just no key yet — that's
//! "available". Anything else (e.g. "no secret-service running") fails the
//! probe and we fall through to the next tier.

use anyhow::{Context, Result};
use keyring::{Entry, Error};

use super::CredStore;

const SERVICE: &str = "gapes";

pub struct KeyringStore;

impl KeyringStore {
    pub fn available() -> bool {
        match Entry::new(SERVICE, "__gapes_availability_probe") {
            Ok(entry) => matches!(entry.get_password(), Err(Error::NoEntry) | Ok(_)),
            Err(_) => false,
        }
    }

    fn entry(server: &str) -> Result<Entry> {
        Entry::new(SERVICE, server).context("keyring entry")
    }
}

impl CredStore for KeyringStore {
    fn tier_name(&self) -> &'static str {
        "keyring"
    }
    fn display_name(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "macOS Keychain"
        } else if cfg!(target_os = "windows") {
            "Windows Credential Manager"
        } else {
            "OS keychain (libsecret)"
        }
    }
    fn security_label(&self) -> &'static str {
        "●●●●● excellent"
    }
    fn store(&self, server: &str, refresh: &str) -> Result<()> {
        Self::entry(server)?
            .set_password(refresh)
            .context("storing refresh in keyring")
    }
    fn load(&self, server: &str) -> Result<Option<String>> {
        match Self::entry(server)?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    }
    fn delete(&self, server: &str) -> Result<()> {
        match Self::entry(server)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    }
}
