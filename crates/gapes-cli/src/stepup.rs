//! Shared step-up driver: post `stepup-init`, render the confirmation URL +
//! QR, poll `stepup-status` every 1.5s until "confirmed" / "expired", return
//! the code to the caller for the destructive call. Used by `rm`, `share
//! --public`, `devices revoke <other>`.

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
/// the route handlers — `page.delete`, `page.visibility_change`,
/// `device.revoke`).
pub async fn drive(
    client: &Client,
    access: &str,
    intent: &str,
    operation: &str,
    target: Option<&str>,
) -> Result<StepUpOutcome> {
    println!("{} destructive operation — requires confirmation", warn_mark());
    println!();

    let init = client.stepup_init(access, operation, target).await?;
    let qr_str = qr::render(&init.verify_url).unwrap_or_default();

    let lines: Vec<String> = vec![
        format!("open on any device:"),
        format!("  {}", cyan(&init.verify_url)),
        String::new(),
        format!("operation: {intent}"),
        format!("expires in: {}s", init.expires_in),
    ];
    println!("{}", boxed("step-up confirmation", &lines, 56));
    println!();
    println!("{}", qr_str);

    println!("waiting…");

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
