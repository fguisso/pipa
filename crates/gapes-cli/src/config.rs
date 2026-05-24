//! Tiny config blob at `~/.config/gapes/config.toml` — currently only
//! remembers the last server the user logged into so they don't have to keep
//! re-typing it. Distinct from the credential store, which holds the refresh
//! token (and lives in keychain / pass / chmod-600 file).

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
}

pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("could not resolve OS config dir")?;
    let p = base.join("gapes");
    if !p.exists() {
        fs::create_dir_all(&p)?;
    }
    Ok(p)
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn load() -> Config {
    let Ok(p) = config_path() else {
        return Config::default();
    };
    let Ok(text) = fs::read_to_string(&p) else {
        return Config::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

pub fn save(cfg: &Config) -> Result<()> {
    let p = config_path()?;
    let text = toml::to_string_pretty(cfg)?;
    fs::write(&p, text)?;
    Ok(())
}
