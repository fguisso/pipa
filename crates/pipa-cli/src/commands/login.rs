//! `pipa login [--server <url>] [--automation] [--label <text>]`
//!
//! Drives the device flow end-to-end:
//!   1. Resolve server URL (flag → config → prompt).
//!   2. POST `device-init` (anonymous — approval gating lives in the browser).
//!   3. Show `verify_url` (a human approves it in a browser; the CLI can't
//!      self-approve). Auto-open + QR in human mode; a JSON line in --json mode.
//!   4. Poll `device-poll` every 2s until Approved or expired (10 min).
//!   5. Stash refresh in the highest-tier credstore available.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use dialoguer::{Input, theme::ColorfulTheme};
use pipa_sdk::{Client, DevicePoll};
use tokio::time::sleep;

use crate::browser;
use crate::cli::LoginArgs;
use crate::config::{self, Config};
use crate::credstore;
use crate::output::{boxed, check, cyan, dim, kv};
use crate::qr;

const POLL_INTERVAL_SECS: u64 = 2;
const POLL_TIMEOUT_SECS: u64 = 600;

pub async fn run(args: LoginArgs, json: bool) -> Result<()> {
    let theme = ColorfulTheme::default();

    let server = match args.server {
        Some(s) => s,
        None => {
            let prev = config::load().server;
            let default = prev.unwrap_or_else(|| "http://127.0.0.1:8080".into());
            if json {
                // No interactive prompt in JSON mode — require an explicit server.
                match config::load().server {
                    Some(s) => s,
                    None => bail!("--server <url> is required in --json mode"),
                }
            } else {
                let s: String = Input::with_theme(&theme)
                    .with_prompt("pipa server URL")
                    .default(default)
                    .interact_text()?;
                s
            }
        }
    };
    let server = server.trim_end_matches('/').to_string();

    let scope = if args.automation { "automation" } else { "interactive" };

    let client = Client::new(&server, None)?;

    let init = client
        .device_init(scope, args.label.as_deref())
        .await
        .context("failed to start device-flow with server")?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "verify_url": init.verify_url,
                "expires_in": init.expires_in,
            })
        );
    } else {
        let opened = browser::try_open(&init.verify_url);
        println!();
        if opened {
            println!("► browser opened — approve on the page that just loaded.");
            println!("  if nothing opened, visit:");
            println!("    {}", cyan(&init.verify_url));
        } else {
            println!("► visit on any device:");
            println!("    {}", cyan(&init.verify_url));
        }
        println!("► or scan:");
        let qr_str = qr::render(&init.verify_url).unwrap_or_default();
        println!("{qr_str}");
        println!("► waiting for approval (polling every {POLL_INTERVAL_SECS}s, expires in 10:00)…");
        println!();
    }

    let started = std::time::Instant::now();
    let approved = loop {
        if started.elapsed().as_secs() > POLL_TIMEOUT_SECS {
            bail!("device-flow timed out without approval");
        }
        let poll = client
            .device_poll(&init.device_code, &init.device_secret)
            .await?;
        match poll {
            DevicePoll::Pending => sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await,
            DevicePoll::Approved {
                refresh_token,
                device_id,
                device_label,
                scope,
                server: srv,
            } => {
                break (refresh_token, device_id, device_label, scope, srv);
            }
        }
    };
    let (refresh, device_id, device_label, scope_final, server_resp) = approved;

    let canonical_server = if server_resp.is_empty() {
        server.clone()
    } else {
        server_resp.trim_end_matches('/').to_string()
    };

    let store = credstore::pick_best();
    store
        .store(&canonical_server, &refresh)
        .context("storing refresh token in credential store")?;

    let cfg = Config {
        server: Some(canonical_server.clone()),
        device_id: Some(device_id.clone()),
    };
    config::save(&cfg).context("writing config.toml")?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "approved",
                "server": canonical_server,
                "device": device_label,
                "device_id": device_id,
                "scope": scope_final,
                "creds": store.display_name(),
            })
        );
        return Ok(());
    }

    println!("{} logged in ({} scope)", check(), scope_final);
    println!("{} device: {}", check(), device_label);
    let inner = vec![
        format!("stored in: {}", store.display_name()),
        format!("security:  {}", store.security_label()),
        String::new(),
        "to revoke this device:".to_string(),
        "  from another logged-in device:".to_string(),
        format!("    pipa devices revoke {}", device_id),
        "  from the server console:".to_string(),
        format!("    pipa-server devices revoke {}", device_id),
    ];
    println!("{}", boxed("credential storage", &inner, 56));
    println!();
    println!("{}", dim("tip:"));
    println!("{}", kv("deploy", "pipa deploy ./dist"));
    println!("{}", kv("list", "pipa ls"));
    println!("{}", kv("help", "pipa --help"));

    Ok(())
}
