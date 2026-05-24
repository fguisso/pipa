//! Tier 2: `pass` (the standard unix password manager). Shells out to the
//! `pass` binary for read/write/delete. We key by the server URL host so a
//! user with multiple pipa servers gets distinct entries (e.g.
//! `pipa/pages.example.com/refresh`).

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use super::CredStore;

pub struct PassStore;

impl PassStore {
    pub fn available() -> bool {
        if which::which("pass").is_err() {
            return false;
        }
        // `pass ls` exits 0 even on an empty store, but errors out if there
        // is no gpg-id (uninitialized). Don't probe the network; just trust
        // the binary's exit code.
        Command::new("pass")
            .arg("ls")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn entry(server: &str) -> String {
        let host = url::Url::parse(server)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_else(|| server.to_string());
        format!("pipa/{host}/refresh")
    }
}

impl CredStore for PassStore {
    fn tier_name(&self) -> &'static str {
        "pass"
    }
    fn display_name(&self) -> &'static str {
        "pass (password-store)"
    }
    fn security_label(&self) -> &'static str {
        "●●●●○ very good"
    }
    fn store(&self, server: &str, refresh: &str) -> Result<()> {
        let entry = Self::entry(server);
        let mut child = Command::new("pass")
            .args(["insert", "-m", "-f", &entry])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn pass insert")?;
        child
            .stdin
            .as_mut()
            .context("pass stdin")?
            .write_all(refresh.as_bytes())?;
        let status = child.wait()?;
        if !status.success() {
            bail!("pass insert failed");
        }
        Ok(())
    }
    fn load(&self, server: &str) -> Result<Option<String>> {
        let entry = Self::entry(server);
        let out = Command::new("pass")
            .arg("show")
            .arg(&entry)
            .stdin(Stdio::null())
            .output()?;
        if !out.status.success() {
            // `pass show` exits 1 on missing entry. Distinguish by looking
            // for "not in the password store" on stderr; otherwise treat as
            // a hard error so we don't pretend a missing decryption key is
            // a missing entry.
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("is not in the password store") {
                return Ok(None);
            }
            bail!("pass show failed: {}", stderr.trim());
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(Some(s))
    }
    fn delete(&self, server: &str) -> Result<()> {
        let entry = Self::entry(server);
        let status = Command::new("pass")
            .args(["rm", "-f", &entry])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            bail!("pass rm failed");
        }
        Ok(())
    }
}
