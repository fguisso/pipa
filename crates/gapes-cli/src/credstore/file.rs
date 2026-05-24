//! Tier 4: chmod-600 TOML file at `~/.config/gapes/auth.toml`. Format is a
//! flat map of `<server_url> -> { refresh = "..." }`.
//!
//! Always available — this is the fallback when nothing else works. On Unix
//! we eagerly chmod the file to 0o600 on every write; on Windows we rely on
//! the user profile's default ACL since file modes don't translate.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::CredStore;
use crate::config::config_dir;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Auth {
    #[serde(default)]
    servers: BTreeMap<String, Entry>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Entry {
    refresh: String,
}

pub struct FileStore {
    path: PathBuf,
}

impl Default for FileStore {
    fn default() -> Self {
        let path = config_dir()
            .map(|d| d.join("auth.toml"))
            .unwrap_or_else(|_| PathBuf::from("./gapes-auth.toml"));
        Self { path }
    }
}

impl FileStore {
    fn read(&self) -> Result<Auth> {
        if !self.path.exists() {
            return Ok(Auth::default());
        }
        let text = fs::read_to_string(&self.path).context("reading auth.toml")?;
        Ok(toml::from_str(&text).unwrap_or_default())
    }
    fn write(&self, auth: &Auth) -> Result<()> {
        let text = toml::to_string_pretty(auth)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, text).context("writing auth.toml")?;
        chmod_600(&self.path)?;
        Ok(())
    }
}

impl CredStore for FileStore {
    fn tier_name(&self) -> &'static str {
        "file"
    }
    fn display_name(&self) -> &'static str {
        "~/.config/gapes/auth.toml (chmod 600)"
    }
    fn security_label(&self) -> &'static str {
        "●●○○○ fallback"
    }
    fn store(&self, server: &str, refresh: &str) -> Result<()> {
        let mut auth = self.read()?;
        auth.servers.insert(
            server.to_string(),
            Entry {
                refresh: refresh.into(),
            },
        );
        self.write(&auth)
    }
    fn load(&self, server: &str) -> Result<Option<String>> {
        let auth = self.read()?;
        Ok(auth.servers.get(server).map(|e| e.refresh.clone()))
    }
    fn delete(&self, server: &str) -> Result<()> {
        let mut auth = self.read()?;
        auth.servers.remove(server);
        if auth.servers.is_empty() && self.path.exists() {
            fs::remove_file(&self.path)?;
            return Ok(());
        }
        self.write(&auth)
    }
}

#[cfg(unix)]
fn chmod_600(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(p)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(p, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn chmod_600(_p: &Path) -> Result<()> {
    Ok(())
}
