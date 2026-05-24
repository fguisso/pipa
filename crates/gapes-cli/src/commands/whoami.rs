//! `gapes whoami` — answer "who am I, what's my scope, where is my token?".
//! Mints a `manage:devices` access to enumerate devices, picks out the one
//! whose id matches our locally-cached `device_id`, and prints the credstore
//! tier that holds the refresh.

use anyhow::Result;

use crate::commands::client_with_access;
use crate::config;
use crate::credstore;
use crate::output::{check, dim, kv};

pub async fn run() -> Result<()> {
    let cfg = config::load();
    let Some(server) = cfg.server.clone() else {
        println!("{} not logged in", dim("·"));
        return Ok(());
    };

    let holder = credstore::find_holder(&server);
    let (tier_name, sec_label) = holder
        .as_ref()
        .map(|h| (h.display_name(), h.security_label()))
        .unwrap_or(("(none)", "—"));

    let result = client_with_access("manage:devices").await;
    let (devices_summary, scope_line) = match result {
        Ok((client, _, access)) => match client.list_devices(&access).await {
            Ok(list) => {
                let me = list
                    .devices
                    .iter()
                    .find(|d| Some(&d.id) == cfg.device_id.as_ref());
                if let Some(d) = me {
                    (
                        format!("{} ({})", d.label, d.id),
                        format!("scope: {}", d.scope),
                    )
                } else if let Some(d) = list.devices.first() {
                    (
                        format!("{} ({})", d.label, d.id),
                        format!("scope: {}", d.scope),
                    )
                } else {
                    ("(no devices?)".into(), "scope: (unknown)".into())
                }
            }
            Err(e) => (format!("(server lookup failed: {e})"), "scope: ?".into()),
        },
        Err(e) => (
            "(could not mint access token)".into(),
            format!("error: {e}"),
        ),
    };

    println!("{} {}", check(), devices_summary);
    println!("{}", kv("server", &server));
    println!("{}", kv(&scope_line.split(':').next().unwrap_or("scope"), scope_line.split_once(": ").map(|(_, v)| v).unwrap_or("?")));
    println!("{}", kv("creds", &format!("{tier_name}  ({sec_label})")));
    Ok(())
}
