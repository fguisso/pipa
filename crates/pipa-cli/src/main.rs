use anyhow::Result;
use clap::Parser;

mod browser;
mod cli;
mod commands;
mod config;
mod credstore;
mod output;
mod qr;
mod stepup;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Cli::parse();
    match args.command {
        cli::Command::Login(c) => commands::login::run(c).await,
        cli::Command::Logout => commands::logout::run().await,
        cli::Command::Whoami => commands::whoami::run().await,
        cli::Command::Deploy(c) => commands::deploy::run(c).await,
        cli::Command::Ls => commands::ls::run().await,
        cli::Command::Get(c) => commands::get::run(c).await,
        cli::Command::Stats(c) => commands::stats::run(c).await,
        cli::Command::Share(c) => commands::share::run(c).await,
        cli::Command::Rm(c) => commands::rm::run(c).await,
        cli::Command::Devices(c) => commands::devices::run(c).await,
        cli::Command::Activity(c) => commands::activity::run(c).await,
        cli::Command::Comments(c) => commands::comments::run(c).await,
    }
}
