//! One module per top-level CLI subcommand.

pub mod activity;
pub mod comments;
pub mod deploy;
pub mod devices;
pub mod get;
pub mod ls;
pub mod login;
pub mod logout;
pub mod rm;
pub mod share;
pub mod stats;
pub mod whoami;

use anyhow::{Context, Result};
use gapes_sdk::Client;

use crate::config;
use crate::credstore;

/// Shared helper: pull the refresh from the credstore and build a Client.
/// Errors with a friendly "run `gapes login`" if no token exists.
pub fn client_from_credstore() -> Result<(Client, String)> {
    let cfg = config::load();
    let server = cfg
        .server
        .clone()
        .context("not logged in — run `gapes login --server <url>`")?;
    let store = credstore::pick_best();
    let refresh = store
        .load(&server)?
        .context("not logged in — run `gapes login`")?;
    let client = Client::new(&server, Some(refresh))?;
    Ok((client, server))
}

/// Build a Client and immediately mint a fresh access token at the given scope.
/// Persists the rotated refresh back to the same credstore tier we read from.
///
/// If the active tier is read-only (e.g. `GAPES_REFRESH_TOKEN`), persistence
/// is silently skipped so CI runs that inject a token still work for one
/// command. The caller is expected to re-inject before the next run, or
/// switch to a writable tier.
pub async fn client_with_access(scope: &str) -> Result<(Client, String, String)> {
    let (mut client, server) = client_from_credstore()?;
    let mint = client
        .mint_access(scope, 300)
        .await
        .context("minting access token")?;
    let store = credstore::pick_best();
    if store.tier_name() != "env" {
        store
            .store(&server, &mint.refresh)
            .context("persisting rotated refresh")?;
    } else {
        eprintln!(
            "warning: GAPES_REFRESH_TOKEN is read-only; rotated refresh = {}",
            mint.refresh
        );
    }
    Ok((client, server, mint.access))
}
