//! `gapes login [--server <url>] [--automation] [--label <text>]`
//!
//! Drives the device flow end-to-end:
//!   1. Resolve server URL (flag → config → prompt).
//!   2. POST `device-init` (anonymous — approval gating lives in the browser).
//!   3. Auto-open `verify_url` in the user's browser; also print URL + QR.
//!   4. Poll `device-poll` every 2s until Approved or expired (10 min).
//!   5. Stash refresh in the highest-tier credstore available.
//!   6. Pretty-print the storage tier so the user knows what protects it.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use dialoguer::{Input, theme::ColorfulTheme};
use gapes_sdk::{Client, DevicePoll};
use tokio::time::sleep;

use crate::browser;
use crate::cli::LoginArgs;
use crate::config::{self, Config};
use crate::credstore;
use crate::output::{boxed, check, cyan, dim, kv};
use crate::qr;

const POLL_INTERVAL_SECS: u64 = 2;
const POLL_TIMEOUT_SECS: u64 = 600;

pub async fn run(args: LoginArgs) -> Result<()> {
    let theme = ColorfulTheme::default();

    let server = match args.server {
        Some(s) => s,
        None => {
            let prev = config::load().server;
            let default = prev.unwrap_or_else(|| "http://127.0.0.1:8080".into());
            let s: String = Input::with_theme(&theme)
                .with_prompt("gapes server URL")
                .default(default)
                .interact_text()?;
            s
        }
    };
    let server = server.trim_end_matches('/').to_string();

    let scope = if args.automation { "automation" } else { "interactive" };

    let client = Client::new(&server, None)?;

    let init = client
        .device_init(scope, args.label.as_deref())
        .await
        .context("failed to start device-flow with server")?;

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

    println!("{} logged in ({} scope)", check(), scope_final);
    println!("{} device: {}", check(), device_label);
    let inner = vec![
        format!("stored in: {}", store.display_name()),
        format!("security:  {}", store.security_label()),
        String::new(),
        format!("to revoke this device:"),
        format!("  from another logged-in device:"),
        format!("    gapes devices revoke {}", device_id),
        format!("  from the server console:"),
        format!("    gapes-server devices revoke {}", device_id),
    ];
    println!("{}", boxed("credential storage", &inner, 56));
    println!();
    println!("{}", dim("tip:"));
    println!("{}", kv("deploy", "gapes deploy ./dist"));
    println!("{}", kv("list", "gapes ls"));
    println!("{}", kv("help", "gapes --help"));

    Ok(())
}
