use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "pipa", version, about = "pipa — deploy static sites you own", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Log in to a pipa server via device flow.
    Login(LoginArgs),
    /// Revoke the current device and wipe the local credential.
    Logout,
    /// Show the current device and credential storage tier.
    Whoami,
    /// Deploy a directory as a new or existing page.
    Deploy(DeployArgs),
    /// List your pages.
    Ls,
    /// Show metadata for a single page.
    Get(GetArgs),
    /// Show per-page analytics.
    Stats(StatsArgs),
    /// Change a page's visibility / password.
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
    /// Public URL of the pipa server (defaults to the last one used).
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
    /// Visibility on create. For updates, omit to keep existing.
    #[arg(long, value_parser = ["private", "public", "password"])]
    pub visibility: Option<String>,
    /// Required if visibility=password.
    #[arg(long)]
    pub password: Option<String>,
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
    #[arg(long, conflicts_with_all = ["private", "password"])]
    pub public: bool,
    #[arg(long, conflicts_with_all = ["public", "password"])]
    pub private: bool,
    #[arg(long)]
    pub password: Option<String>,
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
