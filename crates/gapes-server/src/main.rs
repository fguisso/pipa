use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use gapes_server::{cli, serve};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cli = cli::Cli::parse();
    serve::run(cli).await
}
