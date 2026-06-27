use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "gapes", version, about = "gapes — deploy static sites you own", long_about = None)]
pub struct Cli {
    /// Emit machine-readable JSON to stdout instead of the human/TUI output
    /// (also suppresses spinners, QR codes and colour). Handy for scripts and
    /// AI agents driving the CLI.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Log in to a gapes server via device flow.
    ///
    /// Prints a one-time URL; a human must open it in a browser and approve
    /// the device there (the CLI cannot self-approve). The CLI then polls
    /// until approval and stores the refresh token locally.
    Login(LoginArgs),
    /// Revoke the current device and wipe the local credential.
    Logout,
    /// Show the current device, scope and credential storage tier.
    Whoami,
    /// Show the target server and the optional features it enforces.
    Server,
    /// Explain the access / zone / step-up model (no network call).
    Concepts,
    /// Deploy a directory as a new or existing page.
    Deploy(DeployArgs),
    /// List your pages.
    Ls,
    /// Show metadata for a single page.
    Get(GetArgs),
    /// Show per-page analytics.
    Stats(StatsArgs),
    /// Change a page's access method, zone and/or CSP.
    Share(ShareArgs),
    /// Delete a page (requires step-up confirmation).
    Rm(RmArgs),
    /// List or revoke devices.
    Devices(DevicesArgs),
    /// Recent audit events.
    Activity(ActivityArgs),
    /// Manage comments per page.
    Comments(CommentsArgs),
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Public URL of the gapes server (defaults to the last one used).
    #[arg(long)]
    pub server: Option<String>,
    /// Register as an automation device. Cannot perform destructive ops.
    #[arg(long)]
    pub automation: bool,
    /// Human-readable label to apply to the device.
    #[arg(long)]
    pub label: Option<String>,
}

#[derive(Debug, Args)]
pub struct DeployArgs {
    /// Directory to upload.
    pub dir: PathBuf,
    /// Existing page UUID to update. Omit to create a new page.
    #[arg(long)]
    pub uuid: Option<String>,
    /// Optional human label for the page.
    #[arg(long)]
    pub name: Option<String>,
    /// Hosting mode.
    #[arg(long, value_parser = ["static", "spa"])]
    pub mode: Option<String>,
    /// Auth method on create (the "who can open it" axis). Defaults to
    /// `password` (secure by default). For updates, omit to keep existing.
    #[arg(long, value_parser = ["password", "noauth"])]
    pub access: Option<String>,
    /// Network reach on create (the "where it's reachable" axis): `public`
    /// (internet) or `private` (LAN). Omit to use the server's configured
    /// default. ONLY enforced when the target server has the `zone` feature
    /// (see `gapes server`); against a server without it the CLI refuses
    /// `--zone` unless you also pass `--force`.
    #[arg(long, value_parser = ["public", "private"])]
    pub zone: Option<String>,
    /// Required if access=password.
    #[arg(long)]
    pub password: Option<String>,
    /// Content-Security-Policy strictness. `strict` (default on create) emits
    /// the platform's hardened CSP; `off` suppresses it so the page can load
    /// CDN assets and declare its own policy via `<meta http-equiv>`. On
    /// updates, omit to keep the existing value.
    #[arg(long, value_parser = ["strict", "off"])]
    pub csp: Option<String>,
    /// Send `--zone` even if the server doesn't advertise the `zone` feature
    /// (the value will be stored but NOT enforced — you accept the risk).
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct GetArgs {
    pub uuid: String,
}

#[derive(Debug, Args)]
pub struct StatsArgs {
    pub uuid: String,
    #[arg(long, default_value = "7d", value_parser = ["24h", "7d", "30d", "all"])]
    pub range: String,
}

#[derive(Debug, Args)]
pub struct ShareArgs {
    pub uuid: String,
    /// Change the auth method (the "who" axis). `noauth` removes the gate and
    /// is destructive — it drives a step-up you must confirm in a browser.
    /// Mutually exclusive with `--password`.
    #[arg(long, value_parser = ["password", "noauth"], conflicts_with = "password")]
    pub access: Option<String>,
    /// Change the network reach (the "where" axis). `public` exposes the page
    /// to the internet and is destructive (browser step-up); `private` pins it
    /// to the LAN. ONLY enforced on servers with the `zone` feature
    /// (see `gapes server`); otherwise the CLI refuses unless `--force`.
    #[arg(long, value_parser = ["public", "private"])]
    pub zone: Option<String>,
    /// Set access=password with this secret (rotates the password if already set).
    #[arg(long)]
    pub password: Option<String>,
    /// Per-page CSP knob: `strict` (default) emits the platform CSP, `off`
    /// suppresses it. Non-destructive, no step-up. Can be passed alone or
    /// alongside the other flags.
    #[arg(long, value_parser = ["strict", "off"])]
    pub csp: Option<String>,
    /// Send `--zone` even if the server doesn't advertise the `zone` feature
    /// (stored but NOT enforced).
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct RmArgs {
    pub uuid: String,
}

#[derive(Debug, Args)]
pub struct DevicesArgs {
    #[command(subcommand)]
    pub action: Option<DevicesAction>,
}

#[derive(Debug, Subcommand)]
pub enum DevicesAction {
    /// List devices (default).
    Ls,
    /// Revoke a device by id.
    Revoke { id: String },
}

#[derive(Debug, Args)]
pub struct ActivityArgs {
    #[arg(long, default_value = "7d", value_parser = ["24h", "7d", "30d", "all"])]
    pub range: String,
}

#[derive(Debug, Args)]
pub struct CommentsArgs {
    #[command(subcommand)]
    pub action: CommentsAction,
}

#[derive(Debug, Subcommand)]
pub enum CommentsAction {
    /// Enable comments for a page.
    Enable { uuid: String },
    /// Disable comments for a page.
    Disable { uuid: String },
    /// Toggle "require approval" mode for a page.
    RequireApproval {
        uuid: String,
        #[arg(long, conflicts_with = "off")]
        on: bool,
        #[arg(long, conflicts_with = "on")]
        off: bool,
    },
    /// List comments on a page (including pending/hidden).
    Ls {
        uuid: String,
        #[arg(long, value_parser = ["visible", "pending", "hidden"])]
        status: Option<String>,
    },
    /// Show a single comment.
    Show { id: String },
    /// Approve a pending comment.
    Approve { id: String },
    /// Hide a visible comment.
    Hide { id: String },
    /// Delete a comment.
    Rm { id: String },
}
