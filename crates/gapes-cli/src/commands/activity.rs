//! `gapes activity` — recent audit events as a table.
//!
//! The server does not yet expose a public `/api/activity` route (M7 / web
//! UI work). For now, fall back to a friendly placeholder so the command
//! exists and `--help` is meaningful. When the server adds the endpoint, swap
//! this implementation in.

use anyhow::Result;

use crate::cli::ActivityArgs;
use crate::output::dim;

pub async fn run(_args: ActivityArgs) -> Result<()> {
    println!("{}", dim("activity:"));
    println!(
        "  audit events are recorded server-side but the read endpoint is not exposed yet."
    );
    println!(
        "  view them in the web UI at `/admin/activity` (M7+) or query SQLite directly:"
    );
    println!(
        "    sqlite3 ./data/db.sqlite 'select ts, actor, action, target, success from audit_events order by ts desc limit 50'"
    );
    Ok(())
}
