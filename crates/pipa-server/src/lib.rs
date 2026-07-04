//! pipa-server library surface.
//!
//! This crate also produces the `pipa-server` binary; the library exists to
//! expose a stable test-facing API (`build_router`, `build_app_state_for_test`,
//! and the auth/token helpers) so integration tests can hit the real router
//! without touching the bin entrypoint.
//!
//! The bin (main.rs) re-imports these modules via `pipa_server::*` rather
//! than declaring its own copies, so adding the library doesn't duplicate
//! code.

pub mod auth;
pub mod cli;
pub mod error;
pub mod ip_hash;
pub mod middleware;
pub mod routes;
pub mod serve;
pub mod state;
#[cfg(feature = "thumbnails")]
pub mod thumbnails;

use std::sync::Arc;

use axum::Router;
use pipa_adapters::{
    Config, DiskStorage, HmacKey, SqliteAuthStore, SqliteRepository, open_pool, run_migrations,
};
use pipa_core::{SystemClock, UlidGen};

pub use state::AppState;

/// Public entrypoint into the router used by integration tests. The bin
/// crate (main.rs → serve::run) builds the same router this returns; tests
/// invoke this directly with a custom-built `AppState`.
pub fn build_router(state: AppState) -> Router {
    routes::router(state)
}

/// Build an `AppState` wired against an in-memory SQLite database and a real
/// `DiskStorage` rooted at `pages_dir`. The HMAC key is a deterministic 32-
/// byte buffer so tokens minted in one test process are verifiable inside
/// the same test process; tests that need a different key can swap it on
/// the returned state.
///
/// Only callable from tests (the production bootstrap path in `serve.rs`
/// already does the equivalent setup with disk-backed SQLite + key file).
pub async fn build_app_state_for_test(
    pages_dir: std::path::PathBuf,
    config: Config,
) -> anyhow::Result<AppState> {
    let pool = open_pool("sqlite::memory:").await?;
    run_migrations(&pool).await?;

    let clock = Arc::new(SystemClock);
    let id_gen = Arc::new(UlidGen);

    let repo = Arc::new(SqliteRepository::new(
        pool.clone(),
        clock.clone(),
        id_gen.clone(),
    ));
    let auth = Arc::new(SqliteAuthStore::new(
        pool.clone(),
        clock.clone(),
        id_gen.clone(),
    ));
    let storage = Arc::new(DiskStorage::new(
        pages_dir.clone(),
        pages_dir.join(".trash"),
        pages_dir.join(".staging"),
        id_gen.clone(),
    ));

    // Deterministic key so tokens minted via the helpers below verify against
    // the same state. Tests that need rotation can mint a new state.
    let hmac_key = HmacKey::from_bytes(vec![0x42u8; 32]);

    // Ensure the pages dirs exist so the storage adapter doesn't surprise us.
    std::fs::create_dir_all(&pages_dir)?;
    std::fs::create_dir_all(pages_dir.join(".staging"))?;
    std::fs::create_dir_all(pages_dir.join(".trash"))?;

    Ok(AppState::new(repo, auth, storage, hmac_key, config))
}
