//! `pipa share <uuid> [--access password|noauth] [--zone public|private]
//! [--password <secret>] [--csp strict|off]` — change a page's access method,
//! network zone, and/or CSP knob. Loosening security (access→noauth or
//! zone→public) is destructive and drives step-up; tightening and csp-only
//! edits are straight admin calls.

use anyhow::{Result, bail};

use crate::cli::ShareArgs;
use crate::commands::{client_with_access, ensure_feature};
use crate::output::{check, kv};
use crate::stepup;

pub async fn run(args: ShareArgs, json: bool) -> Result<()> {
    // `--password` implies access=password (and rotates the secret).
    let access: Option<&str> = if args.password.is_some() {
        Some("password")
    } else {
        args.access.as_deref()
    };
    let zone = args.zone.as_deref();

    if access.is_none() && zone.is_none() && args.csp.is_none() {
        bail!("pass at least one of --access, --zone, --password <secret>, or --csp <strict|off>");
    }

    // Loosening security needs destroy:<uuid> + step-up; otherwise admin:<uuid>.
    let loosening = access == Some("noauth") || zone == Some("public");
    let scope = if loosening {
        format!("destroy:{}", args.uuid)
    } else {
        format!("admin:{}", args.uuid)
    };
    let (client, _server, token) = client_with_access(&scope).await?;

    // Refuse `--zone` against a server that doesn't enforce it (before any
    // step-up handoff), unless --force.
    if zone.is_some() {
        ensure_feature(&client, &token, "zone", "--zone", args.force).await?;
    }

    // Guard: --no-wait/--resume only make sense for a loosening change (the only
    // kind that triggers step-up). Refuse them on a plain tightening edit rather
    // than silently ignoring.
    if (args.no_wait || args.resume) && !loosening {
        bail!(
            "--no-wait/--resume only apply to a loosening change (--access noauth or --zone public)"
        );
    }

    const OP: &str = "page.weaken_security";
    let stepup_code = if loosening {
        if args.no_wait {
            stepup::init_no_wait(&client, &token, OP, Some(&args.uuid), json).await?;
            return Ok(());
        }
        let code = if args.resume {
            stepup::resume(&client, OP, Some(&args.uuid)).await?.code
        } else {
            stepup::drive(
                &client,
                &token,
                &format!("LOOSEN security on page {}", args.uuid),
                OP,
                Some(&args.uuid),
                json,
            )
            .await?
            .code
        };
        Some(code)
    } else {
        None
    };

    let view = client
        .set_access(
            &token,
            &args.uuid,
            access,
            zone,
            args.password.as_deref(),
            args.csp.as_deref(),
            stepup_code.as_deref(),
        )
        .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&view)?);
        return Ok(());
    }

    println!("{} updated", check());
    println!("{}", kv("uuid", &view.uuid));
    println!("{}", kv("access", &view.access));
    println!("{}", kv("zone", &view.zone));
    println!("{}", kv("csp", &view.csp));
    Ok(())
}
