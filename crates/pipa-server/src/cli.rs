use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "pipa-server", version, about = "pipa HTTP server")]
pub struct Cli {
    /// Path to pages.toml. Defaults to ./pages.toml.
    #[arg(long, default_value = "./pages.toml", global = true)]
    pub config: PathBuf,

    /// Run with dev-mode relaxed cookie/secure expectations. Refuses to bind
    /// non-loopback addresses. Equivalent to PIPA_DEV=1.
    #[arg(long, env = "PIPA_DEV", global = true)]
    pub dev: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Issue a fresh setup code for first-boot or re-pairing a device.
    Setup,
}
