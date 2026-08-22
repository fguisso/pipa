//! `pipa archive <uuid> [--restore]` — reversible page unpublish/republish.

use anyhow::Result;

use crate::cli::ArchiveArgs;
use crate::commands::client_with_access;
use crate::output::check;

pub async fn run(args: ArchiveArgs, json: bool) -> Result<()> {
    let (client, _server, access) = client_with_access(&format!("admin:{}", args.uuid)).await?;
    let page = client.set_archive(&access, &args.uuid, !args.restore).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&page)?);
        return Ok(());
    }

    let state = if args.restore { "restored" } else { "archived" };
    println!("{} {} {}", check(), state, page.uuid);
    Ok(())
}
