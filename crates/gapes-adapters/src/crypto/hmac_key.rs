use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use rand::RngCore;
use rand::rngs::OsRng;

const KEY_LEN: usize = 32;

#[derive(Clone)]
pub struct HmacKey(Vec<u8>);

impl HmacKey {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

/// Load the HMAC key from `path`, or generate + persist a new one on first
/// boot. Created files are chmod 600 on Unix; on non-Unix the file inherits
/// default ACLs (with a `tracing::warn!`).
pub fn load_or_create(path: &Path) -> Result<HmacKey> {
    if path.exists() {
        let mut buf = Vec::with_capacity(KEY_LEN);
        let mut f = fs::File::open(path)
            .with_context(|| format!("opening HMAC key at {}", path.display()))?;
        f.read_to_end(&mut buf)?;
        if buf.len() != KEY_LEN {
            anyhow::bail!(
                "HMAC key at {} is {} bytes, expected {}",
                path.display(),
                buf.len(),
                KEY_LEN
            );
        }
        return Ok(HmacKey(buf));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating key dir {}", parent.display()))?;
    }

    let mut bytes = vec![0u8; KEY_LEN];
    OsRng.fill_bytes(&mut bytes);

    write_secret_file(path, &bytes)
        .with_context(|| format!("writing new HMAC key at {}", path.display()))?;

    Ok(HmacKey(bytes))
}

#[cfg(unix)]
fn write_secret_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_secret_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    tracing::warn!(
        path = %path.display(),
        "writing HMAC key without unix file permissions - secure the file manually"
    );
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
