//! `pipa login [--server <url>] [--automation] [--label <text>]`
//!            `[--no-wait] [--resume]`
//!
//! Drives the device flow. Approval is always a human-in-browser step — the CLI
//! can never self-approve — so the shape is init → show URL → poll until
//! approved → stash refresh.
//!
//! Two ways to run it:
//!   - **Inline (default):** one command does init + wait + store. Opens a
//!     browser and blocks polling until approval (or 10 min).
//!   - **Split (`--no-wait` then `--resume`):** the first call prints the
//!     approval URL and exits; the second blocks until approval. This is what
//!     agents use — show the human the URL, then wait — with no background shell
//!     scraping the CLI's output.
//!
//! `--headless` (global) suppresses browser-open and interactive prompts.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use dialoguer::{Input, theme::ColorfulTheme};
use pipa_sdk::{Client, DeviceInitResponse, DevicePoll};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use crate::browser;
use crate::cli::LoginArgs;
use crate::config::{self, Config};
use crate::credstore;
use crate::output::{boxed, check, cyan, dim, kv};
use crate::qr;

const POLL_INTERVAL_SECS: u64 = 2;
const POLL_TIMEOUT_SECS: u64 = 600;

/// Approval result carried out of the poll loop.
struct Approved {
    refresh: String,
    device_id: String,
    device_label: String,
    scope: String,
    server: String,
}

/// Pending device-flow session persisted between a `--no-wait` init and a
/// `--resume` wait. Holds the short-lived `device_secret`, so the file is
/// chmod-600 and deleted as soon as the flow completes or expires.
#[derive(Serialize, Deserialize)]
struct Pending {
    server: String,
    device_code: String,
    device_secret: String,
    expires_in: i64,
    created_at: i64,
}

fn pending_path() -> Result<PathBuf> {
    Ok(config::config_dir()?.join("pending-login.json"))
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
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&p, text).context("writing pending-login.json")?;
        chmod_600(&p);
        Ok(())
    }

    fn load() -> Result<Option<Pending>> {
        let p = pending_path()?;
        let Ok(text) = std::fs::read_to_string(&p) else {
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

pub async fn run(args: LoginArgs, json: bool) -> Result<()> {
    let headless = credstore::is_headless();

    if args.resume {
        return resume(json).await;
    }

    let server = resolve_server(&args, json, headless)?;
    let scope = if args.automation {
        "automation"
    } else {
        "interactive"
    };
    let client = Client::new(&server, None)?;
    let init = client
        .device_init(scope, args.label.as_deref())
        .await
        .context("failed to start device-flow with server")?;

    if args.no_wait {
        // Persist the pending session, print the approval URL, and exit. The
        // caller shows the URL to a human, then runs `pipa login --resume`.
        Pending {
            server: server.clone(),
            device_code: init.device_code.clone(),
            device_secret: init.device_secret.clone(),
            expires_in: init.expires_in,
            created_at: now(),
        }
        .save()?;

        if json {
            println!(
                "{}",
                serde_json::json!({
                    "verify_url": init.verify_url,
                    "expires_in": init.expires_in,
                    "next": "pipa login --resume",
                })
            );
        } else {
            println!();
            println!("► approve in a browser (on any device):");
            println!("    {}", cyan(&init.verify_url));
            println!();
            println!("then finish with:  {}", cyan("pipa login --resume"));
        }
        return Ok(());
    }

    // Inline path: show the URL and block until approval.
    present_verify(&init, json, headless);
    let approved = poll_until_approved(&client, &init.device_code, &init.device_secret).await?;
    finish(approved, &server, json)
}

async fn resume(json: bool) -> Result<()> {
    let pending = Pending::load()?
        .context("no pending login — run `pipa login --no-wait` first")?;

    // Reject an expired pending session up front rather than polling a code the
    // server already forgot.
    if now() - pending.created_at > pending.expires_in {
        Pending::clear();
        bail!("pending login expired — run `pipa login --no-wait` again");
    }

    let client = Client::new(&pending.server, None)?;
    let approved = poll_until_approved(&client, &pending.device_code, &pending.device_secret).await;
    // Clear the pending file whether we succeeded or failed — a stale secret
    // shouldn't linger on disk. On a bare timeout the user re-inits anyway.
    let result = approved.and_then(|a| finish(a, &pending.server, json));
    Pending::clear();
    result
}

fn resolve_server(args: &LoginArgs, json: bool, headless: bool) -> Result<String> {
    let server = match &args.server {
        Some(s) => s.clone(),
        None => {
            if json || headless {
                // No interactive prompt without a TTY — require an explicit
                // server (or a remembered one).
                config::load().server.context(
                    "--server <url> is required in --json/--headless mode (no remembered server)",
                )?
            } else {
                let prev = config::load().server;
                let default = prev.unwrap_or_else(|| "http://127.0.0.1:8080".into());
                Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("pipa server URL")
                    .default(default)
                    .interact_text()?
            }
        }
    };
    Ok(server.trim_end_matches('/').to_string())
}

/// Present the approval URL. JSON emits a machine line; headless prints the
/// bare URL (no browser, no QR); interactive opens a browser and draws a QR.
fn present_verify(init: &DeviceInitResponse, json: bool, headless: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "verify_url": init.verify_url,
                "expires_in": init.expires_in,
            })
        );
        return;
    }

    if headless {
        println!();
        println!("► approve in a browser (on any device):");
        println!("    {}", cyan(&init.verify_url));
        println!("► waiting for approval (polling every {POLL_INTERVAL_SECS}s, expires in 10:00)…");
        println!();
        return;
    }

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

async fn poll_until_approved(
    client: &Client,
    device_code: &str,
    device_secret: &str,
) -> Result<Approved> {
    let started = std::time::Instant::now();
    loop {
        if started.elapsed().as_secs() > POLL_TIMEOUT_SECS {
            bail!("device-flow timed out without approval");
        }
        match client.device_poll(device_code, device_secret).await? {
            DevicePoll::Pending => sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await,
            DevicePoll::Approved {
                refresh_token,
                device_id,
                device_label,
                scope,
                server,
            } => {
                return Ok(Approved {
                    refresh: refresh_token,
                    device_id,
                    device_label,
                    scope,
                    server,
                });
            }
        }
    }
}

fn finish(approved: Approved, fallback_server: &str, json: bool) -> Result<()> {
    let canonical_server = if approved.server.is_empty() {
        fallback_server.to_string()
    } else {
        approved.server.trim_end_matches('/').to_string()
    };

    let store = credstore::pick_best()?;
    store
        .store(&canonical_server, &approved.refresh)
        .context("storing refresh token in credential store")?;

    let cfg = Config {
        server: Some(canonical_server.clone()),
        device_id: Some(approved.device_id.clone()),
        // Preserve any active workspace across a re-login.
        active_workspace: config::load().active_workspace,
    };
    config::save(&cfg).context("writing config.toml")?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "approved",
                "server": canonical_server,
                "device": approved.device_label,
                "device_id": approved.device_id,
                "scope": approved.scope,
                "creds": store.display_name(),
            })
        );
        return Ok(());
    }

    println!("{} logged in ({} scope)", check(), approved.scope);
    println!("{} device: {}", check(), approved.device_label);
    let inner = vec![
        format!("stored in: {}", store.display_name()),
        format!("security:  {}", store.security_label()),
        String::new(),
        "to revoke this device:".to_string(),
        "  from another logged-in device:".to_string(),
        format!("    pipa devices revoke {}", approved.device_id),
        "  from the server console:".to_string(),
        format!("    pipa-server devices revoke {}", approved.device_id),
    ];
    println!("{}", boxed("credential storage", &inner, 56));
    println!();
    println!("{}", dim("tip:"));
    println!("{}", kv("deploy", "pipa deploy ./dist"));
    println!("{}", kv("list", "pipa ls"));
    println!("{}", kv("help", "pipa --help"));

    Ok(())
}
