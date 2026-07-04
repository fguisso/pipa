//! External-command credstore — bridge to a password manager's CLI
//! (1Password `op`, Bitwarden `bw`, `pass`, a custom script, …).
//!
//! Two env vars, each a shell command:
//!   - `PIPA_SECRET_GET_CMD` — prints the refresh token to stdout. Examples:
//!     `op read op://Private/pipa/refresh` or `bw get password pipa-refresh`.
//!   - `PIPA_SECRET_SET_CMD` — reads the refresh token on **stdin** and stores
//!     it. Example: a wrapper script that calls `op item edit …`. Optional: if
//!     unset the vault is treated as read-only (rotations are surfaced to the
//!     user instead of persisted, like the `PIPA_REFRESH_TOKEN` tier).
//!
//! The target server URL is exported to the child process as
//! `PIPA_SECRET_SERVER` so a single command template can key items by server.
//!
//! This tier is the ONLY credential source honoured under `--headless`
//! (alongside `PIPA_REFRESH_TOKEN`): in that mode we never touch the OS
//! keychain and never fall back to an on-disk file — a misconfigured vault
//! must fail loudly rather than silently degrade.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use super::CredStore;

const GET_VAR: &str = "PIPA_SECRET_GET_CMD";
const SET_VAR: &str = "PIPA_SECRET_SET_CMD";
const SERVER_ENV: &str = "PIPA_SECRET_SERVER";

pub struct CmdStore;

fn get_cmd() -> Option<String> {
    std::env::var(GET_VAR).ok().filter(|s| !s.trim().is_empty())
}

fn set_cmd() -> Option<String> {
    std::env::var(SET_VAR).ok().filter(|s| !s.trim().is_empty())
}

impl CmdStore {
    /// Available when at least the GET command is configured. A SET command is
    /// optional (read-only vaults are supported).
    pub fn available() -> bool {
        get_cmd().is_some()
    }

    /// True when a SET command is configured — i.e. rotations can be persisted
    /// back to the vault. When false the tier behaves read-only like the
    /// `PIPA_REFRESH_TOKEN` env tier.
    pub fn is_writable() -> bool {
        set_cmd().is_some()
    }
}

/// Run a shell command with the server exported as `PIPA_SECRET_SERVER`,
/// optionally feeding `stdin_data` on stdin, and return its captured stdout.
/// A non-zero exit is an error carrying the command's stderr.
fn run_shell(cmd: &str, server: &str, stdin_data: Option<&str>) -> Result<String> {
    let mut command = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(cmd);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(cmd);
        c
    };
    command
        .env(SERVER_ENV, server)
        .stdin(if stdin_data.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("spawning credential command `{cmd}`"))?;

    if let Some(data) = stdin_data {
        let mut stdin = child
            .stdin
            .take()
            .context("credential command stdin unavailable")?;
        stdin
            .write_all(data.as_bytes())
            .context("writing token to credential command stdin")?;
        // Drop closes the pipe so the child sees EOF.
    }

    let output = child
        .wait_with_output()
        .context("waiting on credential command")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "credential command failed ({}): {}",
            output.status,
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

impl CredStore for CmdStore {
    fn tier_name(&self) -> &'static str {
        "cmd"
    }
    fn display_name(&self) -> &'static str {
        "external command (1Password / Bitwarden CLI)"
    }
    fn security_label(&self) -> &'static str {
        "●●●●● external vault"
    }

    fn store(&self, server: &str, refresh: &str) -> Result<()> {
        let Some(cmd) = set_cmd() else {
            bail!(
                "no {SET_VAR} configured — the external credential vault is read-only. \
                 Set {SET_VAR} to a command that reads the token on stdin to persist rotations."
            );
        };
        run_shell(&cmd, server, Some(refresh))?;
        Ok(())
    }

    fn load(&self, server: &str) -> Result<Option<String>> {
        let Some(cmd) = get_cmd() else {
            return Ok(None);
        };
        let out = run_shell(&cmd, server, None)?;
        let token = out.trim();
        if token.is_empty() {
            Ok(None)
        } else {
            Ok(Some(token.to_string()))
        }
    }

    fn delete(&self, _server: &str) -> Result<()> {
        // The external vault's lifecycle belongs to the user's password manager,
        // not to us — deleting the item on `logout` would be a surprising,
        // possibly destructive side effect. Server-side revocation still runs.
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn get_command_stdout_is_the_token() {
        let out = run_shell("printf 'my-token'", "http://srv", None).unwrap();
        assert_eq!(out, "my-token");
    }

    #[test]
    fn set_command_receives_token_on_stdin_and_server_in_env() {
        // A SET command reads the token on stdin; the server is exported so a
        // template can key items by server. Echo both back to prove wiring.
        let out = run_shell(
            "printf '%s@%s' \"$(cat)\" \"$PIPA_SECRET_SERVER\"",
            "srv-1",
            Some("tok-1"),
        )
        .unwrap();
        assert_eq!(out, "tok-1@srv-1");
    }

    #[test]
    fn nonzero_exit_is_an_error_with_stderr() {
        let err = run_shell("echo boom >&2; exit 7", "x", None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("credential command failed"), "got: {msg}");
        assert!(msg.contains("boom"), "stderr should surface: {msg}");
    }
}
