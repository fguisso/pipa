//! `pipa share <uuid> [--public|--private|--password <secret>]` — change a
//! page's visibility. Going public is destructive and drives step-up; private
//! / password are straight admin calls.

use anyhow::{Result, bail};

use crate::cli::ShareArgs;
use crate::commands::client_with_access;
use crate::output::{check, kv};
use crate::stepup;

pub async fn run(args: ShareArgs) -> Result<()> {
    let n = (args.public as u8) + (args.private as u8) + (args.password.is_some() as u8);
    if n == 0 {
        bail!("pass one of --public, --private, --password <secret>");
    }
    if n > 1 {
        bail!("--public, --private, --password are mutually exclusive");
    }

    if args.public {
        let (client, _server, access) = client_with_access(&format!("destroy:{}", args.uuid)).await?;
        let outcome = stepup::drive(
            &client,
            &access,
            &format!("MAKE PUBLIC page {}", args.uuid),
            "page.visibility_change",
            Some(&args.uuid),
        )
        .await?;
        let view = client
            .set_visibility(&access, &args.uuid, "public", None, Some(&outcome.code))
            .await?;
        println!("{} now public", check());
        println!("{}", kv("uuid", &view.uuid));
        println!("{}", kv("visibility", &view.visibility));
        return Ok(());
    }

    if args.private {
        let (client, _server, access) = client_with_access(&format!("admin:{}", args.uuid)).await?;
        let view = client
            .set_visibility(&access, &args.uuid, "private", None, None)
            .await?;
        println!("{} now private", check());
        println!("{}", kv("uuid", &view.uuid));
        println!("{}", kv("visibility", &view.visibility));
        return Ok(());
    }

    if let Some(pw) = args.password.as_deref() {
        let (client, _server, access) = client_with_access(&format!("admin:{}", args.uuid)).await?;
        let view = client
            .set_visibility(&access, &args.uuid, "password", Some(pw), None)
            .await?;
        println!("{} password protection enabled", check());
        println!("{}", kv("uuid", &view.uuid));
        println!("{}", kv("visibility", &view.visibility));
        return Ok(());
    }

    unreachable!()
}
