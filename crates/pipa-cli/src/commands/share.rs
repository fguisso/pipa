//! `pipa share <uuid> [--public|--private|--password <secret>] [--csp strict|off]`
//! — change a page's visibility and/or CSP knob. Going public is destructive
//! and drives step-up; private / password / csp-only are straight admin calls.

use anyhow::{Result, bail};

use crate::cli::ShareArgs;
use crate::commands::client_with_access;
use crate::output::{check, kv};
use crate::stepup;

pub async fn run(args: ShareArgs) -> Result<()> {
    let visibility_changes =
        (args.public as u8) + (args.private as u8) + (args.password.is_some() as u8);
    if visibility_changes > 1 {
        bail!("--public, --private, --password are mutually exclusive");
    }
    if visibility_changes == 0 && args.csp.is_none() {
        bail!("pass one of --public, --private, --password <secret>, or --csp <strict|off>");
    }

    if args.public {
        // Destructive: needs destroy:<uuid> + step-up.
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
            .set_visibility(
                &access,
                &args.uuid,
                Some("public"),
                None,
                args.csp.as_deref(),
                Some(&outcome.code),
            )
            .await?;
        println!("{} now public", check());
        println!("{}", kv("uuid", &view.uuid));
        println!("{}", kv("visibility", &view.visibility));
        println!("{}", kv("csp", &view.csp));
        return Ok(());
    }

    if args.private {
        let (client, _server, access) = client_with_access(&format!("admin:{}", args.uuid)).await?;
        let view = client
            .set_visibility(
                &access,
                &args.uuid,
                Some("private"),
                None,
                args.csp.as_deref(),
                None,
            )
            .await?;
        println!("{} now private", check());
        println!("{}", kv("uuid", &view.uuid));
        println!("{}", kv("visibility", &view.visibility));
        println!("{}", kv("csp", &view.csp));
        return Ok(());
    }

    if let Some(pw) = args.password.as_deref() {
        let (client, _server, access) = client_with_access(&format!("admin:{}", args.uuid)).await?;
        let view = client
            .set_visibility(
                &access,
                &args.uuid,
                Some("password"),
                Some(pw),
                args.csp.as_deref(),
                None,
            )
            .await?;
        println!("{} password protection enabled", check());
        println!("{}", kv("uuid", &view.uuid));
        println!("{}", kv("visibility", &view.visibility));
        println!("{}", kv("csp", &view.csp));
        return Ok(());
    }

    // csp-only path: visibility flags not set, but `--csp` was supplied.
    if let Some(csp) = args.csp.as_deref() {
        let (client, _server, access) = client_with_access(&format!("admin:{}", args.uuid)).await?;
        let view = client
            .set_visibility(&access, &args.uuid, None, None, Some(csp), None)
            .await?;
        println!("{} csp set to {csp}", check());
        println!("{}", kv("uuid", &view.uuid));
        println!("{}", kv("visibility", &view.visibility));
        println!("{}", kv("csp", &view.csp));
        return Ok(());
    }

    unreachable!()
}
