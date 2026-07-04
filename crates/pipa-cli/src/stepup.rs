//! Shared step-up driver: post `stepup-init`, render the confirmation URL +
//! QR, poll `stepup-status` every 1.5s until "confirmed" / "expired", return
//! the code to the caller for the destructive call. Used by `rm`, `share
//! --public/--noauth`, `devices revoke <other>`.
//!
//! In `json` mode the box + QR are replaced by a single line
//! `{"step_up":{"verify_url":...,"expires_in":...}}` on stdout, so an agent
//! can grab the URL to hand to a human and the rest of stdout stays clean.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use pipa_sdk::Client;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use crate::config;
use crate::output::{boxed, cyan, warn_mark};
use crate::qr;

pub struct StepUpOutcome {
    pub code: String,
}

/// A step-up handshake persisted between a `--no-wait` init and a `--resume`
/// execute. Holds the confirmation `code`; the file is chmod-600 and cleared as
/// soon as the flow completes or expires.
#[derive(Serialize, Deserialize)]
struct Pending {
    operation: String,
    target: Option<String>,
    code: String,
    verify_url: String,
    expires_in: i64,
    created_at: i64,
}

fn pending_path() -> Result<PathBuf> {
    Ok(config::config_dir()?.join("pending-stepup.json"))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Pending {
    fn save(&self) -> Result<()> {
        let p = pending_path()?;
        std::fs::write(&p, serde_json::to_string_pretty(self)?)
            .context("writing pending-stepup.json")?;
        chmod_600(&p);
        Ok(())
    }
    fn load() -> Result<Option<Pending>> {
        let Ok(text) = std::fs::read_to_string(pending_path()?) else {
            return Ok(None);
        };
        Ok(serde_json::from_str(&text).ok())
    }
    fn clear() {
        if let Ok(p) = pending_path() {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(unix)]
fn chmod_600(p: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn chmod_600(_p: &std::path::Path) {}

/// Drive a step-up dance.
///
/// `intent` is a human-readable label printed in the box (e.g. `DELETE page
/// 01HXYZ`). `operation`/`target` are the literals the server expects (see
/// the route handlers — `page.delete`, `page.weaken_security`,
/// `device.revoke`).
pub async fn drive(
    client: &Client,
    access: &str,
    intent: &str,
    operation: &str,
    target: Option<&str>,
    json: bool,
) -> Result<StepUpOutcome> {
    let init = client.stepup_init(access, operation, target).await?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "step_up": {
                    "verify_url": init.verify_url,
                    "expires_in": init.expires_in,
                    "intent": intent,
                }
            })
        );
    } else {
        println!("{} destructive operation — requires confirmation", warn_mark());
        println!();
        let qr_str = qr::render(&init.verify_url).unwrap_or_default();
        let lines: Vec<String> = vec![
            "open on any device:".to_string(),
            format!("  {}", cyan(&init.verify_url)),
            String::new(),
            format!("operation: {intent}"),
            format!("expires in: {}s", init.expires_in),
        ];
        println!("{}", boxed("step-up confirmation", &lines, 56));
        println!();
        println!("{}", qr_str);
        println!("waiting…");
    }

    let deadline =
        std::time::Instant::now() + Duration::from_secs(init.expires_in.max(1) as u64 + 5);
    loop {
        if std::time::Instant::now() > deadline {
            bail!("step-up expired before confirmation");
        }
        let status = client.stepup_status(&init.code).await?;
        match status.status.as_str() {
            "confirmed" => {
                return Ok(StepUpOutcome { code: init.code });
            }
            "expired" => bail!("step-up expired before confirmation"),
            "consumed" => bail!("step-up already consumed (operation must run within seconds of confirmation)"),
            "unknown" => bail!("step-up code became unknown to the server"),
            // "pending" or any future status — keep waiting.
            _ => {}
        }
        sleep(Duration::from_millis(1500)).await;
    }
}

/// Init a step-up, persist it as pending, print the confirmation URL, and
/// return WITHOUT waiting. The caller shows the URL to a human, who approves in
/// a browser, then re-runs the same command with `--resume` to execute. This is
/// the agent-native replacement for backgrounding the CLI in a shell script.
pub async fn init_no_wait(
    client: &Client,
    access: &str,
    operation: &str,
    target: Option<&str>,
    json: bool,
) -> Result<()> {
    let init = client.stepup_init(access, operation, target).await?;
    Pending {
        operation: operation.to_string(),
        target: target.map(str::to_string),
        code: init.code.clone(),
        verify_url: init.verify_url.clone(),
        expires_in: init.expires_in,
        created_at: now(),
    }
    .save()?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "step_up": {
                    "verify_url": init.verify_url,
                    "expires_in": init.expires_in,
                    "next": "re-run the same command with --resume",
                }
            })
        );
    } else {
        println!("{} destructive operation — requires confirmation", warn_mark());
        println!();
        println!("► approve in a browser (on any device):");
        println!("    {}", cyan(&init.verify_url));
        println!();
        println!("then finish by re-running the same command with {}", cyan("--resume"));
    }
    Ok(())
}

/// Resume a `--no-wait` step-up: load the pending handshake (which must match
/// `operation`/`target`), poll until the human confirms, and return the code so
/// the caller can immediately run the destructive op. The pending file is
/// cleared on success and on every terminal failure.
pub async fn resume(
    client: &Client,
    operation: &str,
    target: Option<&str>,
) -> Result<StepUpOutcome> {
    let pending = Pending::load()?
        .context("no pending confirmation — run the same command with --no-wait first")?;

    if pending.operation != operation || pending.target.as_deref() != target {
        bail!(
            "the pending confirmation is for a different operation ({}{}) — \
             run THIS command with --no-wait first",
            pending.operation,
            pending
                .target
                .as_deref()
                .map(|t| format!(" {t}"))
                .unwrap_or_default()
        );
    }
    if now() - pending.created_at > pending.expires_in {
        Pending::clear();
        bail!("pending confirmation expired — run the same command with --no-wait again");
    }

    let deadline =
        std::time::Instant::now() + Duration::from_secs(pending.expires_in.max(1) as u64 + 5);
    loop {
        if std::time::Instant::now() > deadline {
            Pending::clear();
            bail!("step-up expired before confirmation");
        }
        let status = client.stepup_status(&pending.code).await?;
        match status.status.as_str() {
            "confirmed" => {
                Pending::clear();
                return Ok(StepUpOutcome { code: pending.code });
            }
            "expired" => {
                Pending::clear();
                bail!("step-up expired before confirmation");
            }
            "consumed" => {
                Pending::clear();
                bail!("step-up already consumed (operation must run within seconds of confirmation)");
            }
            "unknown" => {
                Pending::clear();
                bail!("step-up code became unknown to the server");
            }
            _ => {}
        }
        sleep(Duration::from_millis(1500)).await;
    }
}
