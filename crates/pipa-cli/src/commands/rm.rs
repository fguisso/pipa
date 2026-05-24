//! `pipa rm <uuid>` — delete with step-up.

use anyhow::Result;

use crate::cli::RmArgs;
use crate::commands::client_with_access;
use crate::output::check;
use crate::stepup;

pub async fn run(args: RmArgs) -> Result<()> {
    let (client, _server, access) = client_with_access(&format!("destroy:{}", args.uuid)).await?;
    let outcome = stepup::drive(
        &client,
        &access,
        &format!("DELETE page {}", args.uuid),
        "page.delete",
        Some(&args.uuid),
    )
    .await?;
    client.delete_page(&access, &args.uuid, &outcome.code).await?;
    println!("{} deleted {}", check(), args.uuid);
    Ok(())
}
