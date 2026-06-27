//! One module per top-level CLI subcommand.

pub mod activity;
pub mod comments;
pub mod concepts;
pub mod deploy;
pub mod devices;
pub mod get;
pub mod ls;
pub mod login;
pub mod logout;
pub mod rm;
pub mod server;
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

/// Guard a feature-dependent flag (e.g. `--zone`) against the target server's
/// advertised capabilities (`GET /api/meta`). When the server doesn't enforce
/// `feature`, refuse unless `force` is set — so a value the server would
/// silently ignore can't give a false sense of security. `force` skips the
/// check entirely (no network call).
pub async fn ensure_feature(
    client: &Client,
    access: &str,
    feature: &str,
    flag: &str,
    force: bool,
) -> Result<()> {
    if force {
        return Ok(());
    }
    let features = client
        .meta(access)
        .await
        .map(|m| m.features)
        .context("querying server capabilities (/api/meta)")?;
    if !features.iter().any(|f| f == feature) {
        anyhow::bail!(
            "this server does not enforce `{feature}` — `{flag}` would be stored but ignored. \
             Re-run with --force to send it anyway, or check `gapes server`."
        );
    }
    Ok(())
}
