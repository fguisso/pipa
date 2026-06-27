//! Shared step-up driver: post `stepup-init`, render the confirmation URL +
//! QR, poll `stepup-status` every 1.5s until "confirmed" / "expired", return
//! the code to the caller for the destructive call. Used by `rm`, `share
//! --public/--noauth`, `devices revoke <other>`.
//!
//! In `json` mode the box + QR are replaced by a single line
//! `{"step_up":{"verify_url":...,"expires_in":...}}` on stdout, so an agent
//! can grab the URL to hand to a human and the rest of stdout stays clean.

use std::time::Duration;

use anyhow::{Result, bail};
use gapes_sdk::Client;
use tokio::time::sleep;

use crate::output::{boxed, cyan, warn_mark};
use crate::qr;

pub struct StepUpOutcome {
    pub code: String,
}

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
