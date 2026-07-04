use anyhow::Result;
use clap::Parser;

mod browser;
mod cli;
mod commands;
mod config;
mod credstore;
mod manifest;
mod output;
mod qr;
mod stepup;

#[tokio::main]
async fn main() {
    // Print errors via `Display` (the full `: `-joined cause chain) rather than
    // anyhow's `Debug`, which appends a backtrace whenever `RUST_BACKTRACE` is
    // set in the user's environment. Our friendly errors (e.g. the deploy
    // `--uuid` guidance) should stay clean regardless of ambient env.
    if let Err(e) = run().await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = cli::Cli::parse();
    let json = args.json;
    // Publish the headless switch before any command runs so the credential
    // cascade (and login) can consult it without threading a flag everywhere.
    credstore::set_headless(args.headless);
    match args.command {
        cli::Command::Login(c) => commands::login::run(c, json).await,
        cli::Command::Logout => commands::logout::run().await,
        cli::Command::Whoami => commands::whoami::run(json).await,
        cli::Command::Server => commands::server::run(json).await,
        cli::Command::Concepts => commands::concepts::run(json).await,
        cli::Command::Deploy(c) => commands::deploy::run(c, json).await,
        cli::Command::Ls => commands::ls::run(json).await,
        cli::Command::Get(c) => commands::get::run(c, json).await,
        cli::Command::Stats(c) => commands::stats::run(c, json).await,
        cli::Command::Share(c) => commands::share::run(c, json).await,
        cli::Command::Rm(c) => commands::rm::run(c, json).await,
        cli::Command::Devices(c) => commands::devices::run(c, json).await,
        cli::Command::Activity(c) => commands::activity::run(c).await,
        cli::Command::Comments(c) => commands::comments::run(c).await,
        cli::Command::Workspace(c) => commands::workspace::run(c, json).await,
        cli::Command::Transfer(c) => commands::transfer::run(c, json).await,
    }
}
