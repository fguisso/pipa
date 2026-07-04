//! `pipa devices [ls|revoke <id>]`.

use anyhow::{Result, bail};
use tabled::settings::Style;
use tabled::{Table, Tabled};

use crate::cli::{DevicesAction, DevicesArgs};
use crate::commands::client_with_access;
use crate::config;
use crate::output::{check, dim, fmt_ts};
use crate::stepup;

#[derive(Tabled)]
struct Row {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "LABEL")]
    label: String,
    #[tabled(rename = "SCOPE")]
    scope: String,
    #[tabled(rename = "CREATED")]
    created: String,
    #[tabled(rename = "LAST SEEN")]
    last_seen: String,
    #[tabled(rename = "STATE")]
    state: String,
    #[tabled(rename = "")]
    you: String,
}

pub async fn run(args: DevicesArgs, json: bool) -> Result<()> {
    let action = args.action.unwrap_or(DevicesAction::Ls);
    let cfg = config::load();
    let me = cfg.device_id.clone();

    match action {
        DevicesAction::Ls => {
            let (client, _server, access) = client_with_access("manage:devices").await?;
            let list = client.list_devices(&access).await?;
            if list.devices.is_empty() {
                println!("(no devices?)");
                return Ok(());
            }
            let rows: Vec<Row> = list
                .devices
                .iter()
                .map(|d| Row {
                    id: d.id.clone(),
                    label: d.label.clone(),
                    scope: d.scope.clone(),
                    created: fmt_ts(d.created_at),
                    last_seen: d
                        .last_seen_at
                        .map(fmt_ts)
                        .unwrap_or_else(|| "—".into()),
                    state: if d.revoked_at.is_some() { "revoked".into() } else { "active".into() },
                    you: if Some(&d.id) == me.as_ref() { dim("← you") } else { String::new() },
                })
                .collect();
            let mut table = Table::new(rows);
            table.with(Style::modern_rounded());
            println!("{table}");
        }
        DevicesAction::Revoke {
            id,
            no_wait,
            resume,
        } => {
            let (client, _server, access) = client_with_access("manage:devices").await?;
            let is_self = Some(&id) == me.as_ref();
            const OP: &str = "device.revoke";

            // Revoking your own device is not destructive to others and needs no
            // confirmation — the step-up flags don't apply.
            if is_self {
                if no_wait || resume {
                    bail!("--no-wait/--resume only apply when revoking ANOTHER device");
                }
                client.revoke_device(&access, &id, None).await?;
                println!("{} revoked {}", check(), id);
                return Ok(());
            }

            if no_wait {
                stepup::init_no_wait(&client, &access, OP, Some(&id), json).await?;
                return Ok(());
            }
            let code = if resume {
                stepup::resume(&client, OP, Some(&id)).await?.code
            } else {
                stepup::drive(
                    &client,
                    &access,
                    &format!("REVOKE device {id}"),
                    OP,
                    Some(&id),
                    json,
                )
                .await?
                .code
            };
            client.revoke_device(&access, &id, Some(&code)).await?;
            println!("{} revoked {}", check(), id);
        }
    }
    Ok(())
}
