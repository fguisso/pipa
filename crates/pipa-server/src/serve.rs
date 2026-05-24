use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use pipa_adapters::{
    DiskStorage, SqliteAuthStore, SqliteRepository, crypto::hmac_key, load_config, open_pool,
    run_migrations,
};
use pipa_core::{SystemClock, UlidGen};

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
    if let Some(Command::Setup) = cli.command {
        let setup = auth.issue_setup_code().await?;
        let mins = (setup.expires_at - setup.created_at) / 60;
        println!("[pages] setup code: {}  (expires in {:02}:00)", setup.code, mins);
        return Ok(());
    }

    let state = AppState::new(repo, auth.clone(), storage, hmac_key, config.clone());

    bootstrap_if_first_boot(&state, &data_dir).await?;

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
    tracing::info!("pipa-server listening on {}", addr);

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

async fn bootstrap_if_first_boot(state: &AppState, data_dir: &PathBuf) -> Result<()> {
    let devices = state.auth.devices_count().await?;
    if devices > 0 {
        return Ok(());
    }
    let code = state.auth.issue_setup_code().await?;
    let mins = (code.expires_at - code.created_at) / 60;
    println!("[pages] no devices registered.");
    println!(
        "[pages] setup code: {}  (expires in {:02}:00)",
        code.code, mins
    );
    println!("[pages] from your laptop, run:");
    println!(
        "[pages]   pages login --server {}",
        state.config.server.public_url
    );
    println!("[pages]   when prompted, enter the code above.");

    let setup_path = data_dir.join(".setup-code");
    write_setup_code(&setup_path, &code.code)?;
    Ok(())
}

#[cfg(unix)]
fn write_setup_code(path: &PathBuf, code: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("writing setup code to {}", path.display()))?;
    writeln!(f, "{code}")?;
    Ok(())
}

#[cfg(not(unix))]
fn write_setup_code(path: &PathBuf, code: &str) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("writing setup code to {}", path.display()))?;
    writeln!(f, "{code}")?;
    tracing::warn!(
        path = %path.display(),
        "wrote setup code without unix file permissions - secure manually"
    );
    Ok(())
}
