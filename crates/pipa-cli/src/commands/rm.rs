//! `pipa rm <uuid>` — delete with step-up.
//!
//! Three shapes share one destructive call:
//!   - inline (default): confirm + delete in one blocking command;
//!   - `--no-wait`: print the confirmation URL and exit;
//!   - `--resume`: wait for the human's confirmation, then delete.

use anyhow::Result;

use crate::cli::RmArgs;
use crate::commands::client_with_access;
use crate::output::check;
use crate::stepup;

const OP: &str = "page.delete";

pub async fn run(args: RmArgs, json: bool) -> Result<()> {
    let uuid = &args.uuid;
    let (client, _server, access) = client_with_access(&format!("destroy:{uuid}")).await?;

    if args.no_wait {
        stepup::init_no_wait(&client, &access, OP, Some(uuid), json).await?;
        return Ok(());
    }

    let code = if args.resume {
        stepup::resume(&client, OP, Some(uuid)).await?.code
    } else {
        stepup::drive(
            &client,
            &access,
            &format!("DELETE page {uuid}"),
            OP,
            Some(uuid),
            json,
        )
        .await?
        .code
    };

    client.delete_page(&access, uuid, &code).await?;
    println!("{} deleted {}", check(), uuid);
    Ok(())
}
