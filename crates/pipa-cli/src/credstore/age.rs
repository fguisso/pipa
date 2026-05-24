//! Tier 3: `age`-encrypted file. **Not wired in this milestone** —
//! `available()` always returns false so we never silently pick a path that
//! hasn't been hardened. The struct exists so the cascade can list it for
//! `whoami` once we ship the real implementation.
//!
//! TODO(M7+): wire ssh-agent or `PIPA_AGE_KEY` for decryption, write
//! `~/.config/pipa/auth.age` on store.

use anyhow::{Result, bail};

use super::CredStore;

pub struct AgeStore;

impl AgeStore {
    pub fn available() -> bool {
        false
    }
}

impl CredStore for AgeStore {
    fn tier_name(&self) -> &'static str {
        "age"
    }
    fn display_name(&self) -> &'static str {
        "age-encrypted file (not implemented)"
    }
    fn security_label(&self) -> &'static str {
        "●●●●○ very good"
    }
    fn store(&self, _server: &str, _refresh: &str) -> Result<()> {
        bail!("age credstore is not implemented yet")
    }
    fn load(&self, _server: &str) -> Result<Option<String>> {
        Ok(None)
    }
    fn delete(&self, _server: &str) -> Result<()> {
        Ok(())
    }
}
