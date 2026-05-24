use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};

/// Open a SQLite pool with PRAGMA settings tuned for our workload:
/// WAL + NORMAL is the standard "durable enough, much faster than full".
pub async fn open_pool(url: &str) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(url)
        .with_context(|| format!("parsing sqlite url {url}"))?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await
        .with_context(|| format!("connecting to sqlite at {url}"))?;
    Ok(pool)
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("running sqlite migrations")?;
    Ok(())
}
