use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "gapes-server", version, about = "gapes HTTP server")]
pub struct Cli {
    /// Path to pages.toml. Defaults to ./pages.toml.
    #[arg(long, default_value = "./pages.toml", global = true)]
    pub config: PathBuf,

    /// Run with dev-mode relaxed cookie/secure expectations. Refuses to bind
    /// non-loopback addresses. Equivalent to GAPES_DEV=1.
    #[arg(long, env = "GAPES_DEV", global = true)]
    pub dev: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Revoke every owner session so a different browser can re-claim the
    /// server via `/setup`. Use when you lost access to the original browser
    /// (cookie wiped, device gone). Does NOT revoke CLI devices — they keep
    /// working with their refresh tokens.
    ResetClaim,
}
