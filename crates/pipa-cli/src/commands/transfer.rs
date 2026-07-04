//! `pipa transfer <uuid> <workspace>` — move a page to another workspace.

use anyhow::Result;

use crate::cli::TransferArgs;
use crate::commands::client_with_access;
use crate::output::{check, kv};

pub async fn run(args: TransferArgs, json: bool) -> Result<()> {
    let (client, _s, access) = client_with_access(&format!("admin:{}", args.uuid)).await?;
    let view = client
        .transfer_page(&access, &args.uuid, &args.workspace)
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&view)?);
        return Ok(());
    }
    println!("{} transferred", check());
    println!("{}", kv("uuid", &view.uuid));
    println!("{}", kv("owner", &format!("{} {}", view.owner_kind, view.owner_id)));
    Ok(())
}
