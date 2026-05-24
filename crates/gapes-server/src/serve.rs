use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use gapes_adapters::{
    DiskStorage, SqliteAuthStore, SqliteRepository, crypto::hmac_key, load_config, open_pool,
    run_migrations,
};
use gapes_core::{SystemClock, UlidGen};

use crate::cli::{Cli, Command};
use crate::routes;
use crate::state::{AppState, DynAuthStore, DynRepository, DynStorage};

pub async fn run(cli: Cli) -> Result<()> {
    let mut config = load_config(&cli.config)?;
    config.dev_mode_overlay(cli.dev)?;

    let data_dir = config.server.data_dir.clone();
    let pages_dir = config.server.pages_dir.clone();
    ensure_dirs(&data_dir, &pages_dir)?;

    let db_path = data_dir.join("db.sqlite");
    let db_url = format!("sqlite://{}", db_path.display());
    let pool = open_pool(&db_url).await?;
    run_migrations(&pool).await?;

    let clock = Arc::new(SystemClock);
    let id_gen = Arc::new(UlidGen);

    let repo: DynRepository =
        Arc::new(SqliteRepository::new(pool.clone(), clock.clone(), id_gen.clone()));
    let auth: DynAuthStore =
        Arc::new(SqliteAuthStore::new(pool.clone(), clock.clone(), id_gen.clone()));
    let storage: DynStorage = Arc::new(DiskStorage::new(
        pages_dir.clone(),
        pages_dir.join(".trash"),
        pages_dir.join(".staging"),
        id_gen.clone(),
    ));

    let hmac_path = data_dir.join(".keys").join("hmac.key");
    let hmac_key = hmac_key::load_or_create(&hmac_path)
        .with_context(|| format!("loading HMAC key at {}", hmac_path.display()))?;

    // Handle the one subcommand here so it can reuse the same DB connection.
    if let Some(Command::ResetClaim) = cli.command {
        let sessions = auth.list_owner_sessions().await?;
        let mut revoked = 0u32;
        for s in sessions {
            if s.revoked_at.is_some() {
                continue;
            }
            auth.revoke_owner_session(&s.id).await?;
            revoked += 1;
        }
        auth.delete_admin().await?;
        println!("[gapes] revoked {revoked} owner session(s) and removed the admin user.");
        println!("[gapes] open /setup in your browser to create a new admin.");
        return Ok(());
    }

    let state = AppState::new(repo, auth.clone(), storage, hmac_key, config.clone());

    announce_first_boot(&state).await?;

    if state.config.server.dev {
        tracing::warn!(
            "[DEV MODE] cookies are not marked Secure, do not expose this server"
        );
    }

    let addr: SocketAddr = state
        .config
        .server
        .addr
        .parse()
        .with_context(|| format!("parsing server.addr {}", state.config.server.addr))?;

    let app = routes::router(state.clone());

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!("gapes-server listening on {}", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("axum::serve")?;

    Ok(())
}

fn ensure_dirs(data_dir: &PathBuf, pages_dir: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("creating data_dir {}", data_dir.display()))?;
    std::fs::create_dir_all(data_dir.join(".keys"))
        .with_context(|| format!("creating data_dir/.keys {}", data_dir.display()))?;
    std::fs::create_dir_all(pages_dir)
        .with_context(|| format!("creating pages_dir {}", pages_dir.display()))?;
    std::fs::create_dir_all(pages_dir.join(".staging"))
        .with_context(|| format!("creating pages_dir/.staging {}", pages_dir.display()))?;
    std::fs::create_dir_all(pages_dir.join(".trash"))
        .with_context(|| format!("creating pages_dir/.trash {}", pages_dir.display()))?;
    Ok(())
}

async fn announce_first_boot(state: &AppState) -> Result<()> {
    let admins = state.auth.count_admins().await?;
    if admins > 0 {
        return Ok(());
    }
    let url = state.config.server.public_url.trim_end_matches('/');
    println!("[gapes] no admin yet.");
    println!("[gapes] open {url}/setup in your browser to create your admin account.");
    println!("[gapes] then run `gapes login --server {url}` from any machine.");
    Ok(())
}
