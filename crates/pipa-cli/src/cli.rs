use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "pipa", version, about = "pipa — deploy static sites you own", long_about = None)]
pub struct Cli {
    /// Emit machine-readable JSON to stdout instead of the human/TUI output
    /// (also suppresses spinners, QR codes and colour). Handy for scripts and
    /// AI agents driving the CLI.
    #[arg(long, global = true)]
    pub json: bool,

    /// Non-interactive mode for CI, agents and containers. Never touches the OS
    /// keychain and never falls back to an on-disk credential file — credentials
    /// must come from `PIPA_SECRET_GET_CMD`/`_SET_CMD` (1Password/Bitwarden) or
    /// `PIPA_REFRESH_TOKEN`. Also suppresses browser-open and interactive
    /// prompts (so a blocked keyring or a missing TTY can never hang the CLI).
    #[arg(long, global = true)]
    pub headless: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Log in to a pipa server via device flow.
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
    /// Manage workspaces and membership (Phase 4).
    Workspace(WorkspaceArgs),
    /// Move a page to another workspace.
    Transfer(TransferArgs),
}

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub action: WorkspaceAction,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceAction {
    /// List the workspaces you belong to and your role in each.
    Ls,
    /// Set the active workspace for `deploy` (persisted in config).
    Use { id: String },
    /// Clear the active workspace (deploys go to your personal workspace).
    Unset,
    /// Create a new team workspace (you become its owner).
    Create { name: String },
    /// Show a workspace's members and quota (defaults to the active one).
    Show { id: Option<String> },
    /// Add a member by username.
    MemberAdd {
        ws: String,
        username: String,
        #[arg(long, default_value = "viewer", value_parser = ["owner", "admin", "editor", "viewer"])]
        role: String,
    },
    /// Change a member's role.
    MemberRole {
        ws: String,
        user_id: String,
        #[arg(value_parser = ["owner", "admin", "editor", "viewer"])]
        role: String,
    },
    /// Remove a member.
    MemberRm { ws: String, user_id: String },
    /// Set (or clear) a workspace's page/byte quota. Omit a flag to leave it.
    Quota {
        ws: String,
        /// Max pages (`0` to forbid, omit to leave unchanged).
        #[arg(long)]
        max_pages: Option<i64>,
        /// Max total bytes.
        #[arg(long)]
        max_bytes: Option<i64>,
    },
}

#[derive(Debug, Args)]
pub struct TransferArgs {
    pub uuid: String,
    /// Destination workspace id.
    pub workspace: String,
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
    /// Start the flow, print the approval URL, and exit WITHOUT waiting.
    /// Pair with `pipa login --resume` to block until approval. Lets an agent
    /// show the URL to a human and then wait — no background shell needed.
    #[arg(long, conflicts_with = "resume")]
    pub no_wait: bool,
    /// Resume a `--no-wait` login: block polling until the pending approval
    /// completes, then store the credential. Ignores the connection flags
    /// (server/scope come from the pending session).
    #[arg(long)]
    pub resume: bool,
}

#[derive(Debug, Args)]
pub struct DeployArgs {
    /// Directory to upload.
    pub dir: PathBuf,
    /// Existing page UUID to update. Omit to reuse the page remembered for this
    /// directory (see the deploy manifest), or to create a new one if none.
    #[arg(long)]
    pub uuid: Option<String>,
    /// Force-create a fresh page, ignoring any page remembered for this
    /// directory. Mutually exclusive with `--uuid`.
    #[arg(long, conflicts_with = "uuid")]
    pub new: bool,
    /// Workspace to create the page in (Phase 4). Overrides the active
    /// workspace set via `pipa workspace use`. Ignored when updating an
    /// existing page. Defaults to your personal workspace.
    #[arg(long)]
    pub workspace: Option<String>,
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
    /// (see `pipa server`); against a server without it the CLI refuses
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
    /// (see `pipa server`); otherwise the CLI refuses unless `--force`.
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
    /// For a loosening change (needs step-up): start the confirmation, print the
    /// URL, and exit without waiting. Re-run the SAME command with `--resume` to
    /// finish. No effect on non-loosening edits.
    #[arg(long, conflicts_with = "resume")]
    pub no_wait: bool,
    /// Resume a `--no-wait` loosening: wait for the human's confirmation, then
    /// apply the change. Re-run with the same loosening flags.
    #[arg(long)]
    pub resume: bool,
}

#[derive(Debug, Args)]
pub struct RmArgs {
    pub uuid: String,
    /// Start the step-up confirmation, print the URL, and exit without waiting.
    /// Re-run with `--resume` to finish. Lets an agent hand the URL to a human
    /// with no background shell.
    #[arg(long, conflicts_with = "resume")]
    pub no_wait: bool,
    /// Resume a `--no-wait` deletion: wait for the human's confirmation, then
    /// delete.
    #[arg(long)]
    pub resume: bool,
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
    Revoke {
        id: String,
        /// Revoking another device needs step-up: start the confirmation, print
        /// the URL, and exit without waiting. Re-run with `--resume` to finish.
        #[arg(long, conflicts_with = "resume")]
        no_wait: bool,
        /// Resume a `--no-wait` revoke: wait for confirmation, then revoke.
        #[arg(long)]
        resume: bool,
    },
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
