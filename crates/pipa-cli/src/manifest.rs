//! Per-directory deploy memory.
//!
//! After a successful `pipa deploy <dir>`, we remember `<absolute dir> → uuid`
//! in a central TOML file at `~/.config/pipa/deploy-manifest.toml`. A later
//! `pipa deploy <dir>` with no `--uuid` then *updates* that page instead of
//! silently creating a duplicate every time.
//!
//! The file lives in the config dir, NOT inside the deploy directory — a
//! dotfile under `<dir>` would get swept into the next zip and uploaded.
//!
//! Precedence: explicit `--uuid` overrides the manifest; `--new` ignores it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::config_dir;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// Keyed by absolute, canonicalized deploy-directory path.
    #[serde(default)]
    entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub uuid: String,
    #[serde(default)]
    pub url: String,
    pub updated_at: i64,
}

fn manifest_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("deploy-manifest.toml"))
}

fn key(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Best-effort load — a missing or corrupt file yields an empty manifest rather
/// than failing a deploy over analytics-grade convenience state.
pub fn load() -> Manifest {
    let Ok(p) = manifest_path() else {
        return Manifest::default();
    };
    let Ok(text) = fs::read_to_string(&p) else {
        return Manifest::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

impl Manifest {
    pub fn get(&self, dir: &Path) -> Option<&Entry> {
        self.entries.get(&key(dir))
    }

    pub fn remember(&mut self, dir: &Path, uuid: String, url: String) {
        self.entries.insert(
            key(dir),
            Entry {
                uuid,
                url,
                updated_at: now(),
            },
        );
    }

    pub fn forget(&mut self, dir: &Path) {
        self.entries.remove(&key(dir));
    }

    pub fn save(&self) -> Result<()> {
        let p = manifest_path()?;
        let text = toml::to_string_pretty(self).context("serialize deploy manifest")?;
        fs::write(&p, text).context("write deploy manifest")?;
        Ok(())
    }
}
