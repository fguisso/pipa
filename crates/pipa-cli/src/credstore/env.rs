use anyhow::{Result, bail};

use super::CredStore;

const ENV_VAR: &str = "PIPA_REFRESH_TOKEN";

/// Read-only credstore backed by the `PIPA_REFRESH_TOKEN` env var. Used by
/// CI / containerized agents that inject a token at boot. `store` and
/// `delete` are no-ops with a clear error — we never mutate the environment.
pub struct EnvStore;

impl EnvStore {
    pub fn available() -> bool {
        std::env::var(ENV_VAR).map(|s| !s.is_empty()).unwrap_or(false)
    }
}

impl CredStore for EnvStore {
    fn tier_name(&self) -> &'static str {
        "env"
    }
    fn display_name(&self) -> &'static str {
        "PIPA_REFRESH_TOKEN env var"
    }
    fn security_label(&self) -> &'static str {
        "●●○○○ ci-only"
    }
    fn store(&self, _server: &str, _refresh: &str) -> Result<()> {
        bail!("PIPA_REFRESH_TOKEN is read-only; unset it to use a writable credstore")
    }
    fn load(&self, _server: &str) -> Result<Option<String>> {
        Ok(std::env::var(ENV_VAR).ok().filter(|s| !s.is_empty()))
    }
    fn delete(&self, _server: &str) -> Result<()> {
        bail!("PIPA_REFRESH_TOKEN is read-only; unset it manually")
    }
}
