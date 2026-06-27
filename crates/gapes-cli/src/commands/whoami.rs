//! `gapes whoami` — answer "who am I, what's my scope, where is my token?".
//! Mints a `manage:devices` access to enumerate devices, picks out the one
//! whose id matches our locally-cached `device_id`, and prints the credstore
//! tier that holds the refresh. With --json, emits a machine-readable status
//! (including `logged_in`) so scripts/agents can branch on it.

use anyhow::Result;

use crate::commands::client_with_access;
use crate::config;
use crate::credstore;
use crate::output::{check, dim, kv};

pub async fn run(json: bool) -> Result<()> {
    let cfg = config::load();
    let Some(server) = cfg.server.clone() else {
        if json {
            println!("{}", serde_json::json!({ "logged_in": false }));
        } else {
            println!("{} not logged in", dim("·"));
        }
        return Ok(());
    };

    let holder = credstore::find_holder(&server);
    let (tier_name, sec_label) = holder
        .as_ref()
        .map(|h| (h.display_name(), h.security_label()))
        .unwrap_or(("(none)", "—"));

    let result = client_with_access("manage:devices").await;
    let (device, scope) = match result {
        Ok((client, _, access)) => match client.list_devices(&access).await {
            Ok(list) => {
                let me = list
                    .devices
                    .iter()
                    .find(|d| Some(&d.id) == cfg.device_id.as_ref())
                    .or_else(|| list.devices.first());
                if let Some(d) = me {
                    (format!("{} ({})", d.label, d.id), d.scope.to_string())
                } else {
                    ("(no devices?)".into(), "(unknown)".into())
                }
            }
            Err(e) => (format!("(server lookup failed: {e})"), "?".into()),
        },
        Err(e) => ("(could not mint access token)".into(), format!("error: {e}")),
    };

    if json {
        println!(
            "{}",
            serde_json::json!({
                "logged_in": true,
                "server": server,
                "device": device,
                "scope": scope,
                "creds": tier_name,
            })
        );
        return Ok(());
    }

    println!("{} {}", check(), device);
    println!("{}", kv("server", &server));
    println!("{}", kv("scope", &scope));
    println!("{}", kv("creds", &format!("{tier_name}  ({sec_label})")));
    Ok(())
}
