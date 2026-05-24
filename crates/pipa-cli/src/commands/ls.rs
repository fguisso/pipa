//! `pipa ls` — list pages owned by this account.

use anyhow::Result;
use tabled::settings::{Style, object::Rows};
use tabled::{Table, Tabled};

use crate::commands::client_with_access;
use crate::output::{fmt_ts, human_bytes};

#[derive(Tabled)]
struct Row {
    #[tabled(rename = "UUID")]
    uuid: String,
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "VIS")]
    vis: String,
    #[tabled(rename = "MODE")]
    mode: String,
    #[tabled(rename = "SIZE")]
    size: String,
    #[tabled(rename = "FILES")]
    files: String,
    #[tabled(rename = "UPDATED")]
    updated: String,
}

pub async fn run() -> Result<()> {
    let (client, _server, access) = client_with_access("read:*").await?;
    let resp = client.list_pages(&access).await?;

    if resp.pages.is_empty() {
        println!("no pages yet — `pipa deploy <dir>` to create one");
        return Ok(());
    }

    let rows: Vec<Row> = resp
        .pages
        .iter()
        .map(|p| Row {
            uuid: p.uuid.clone(),
            name: p.name.clone().unwrap_or_else(|| "—".into()),
            vis: p.visibility.clone(),
            mode: p.mode.clone(),
            size: human_bytes(p.size_bytes),
            files: p.file_count.to_string(),
            updated: fmt_ts(p.updated_at),
        })
        .collect();

    let mut table = Table::new(rows);
    table.with(Style::modern_rounded()).modify(Rows::first(), tabled::settings::Alignment::left());
    println!("{table}");
    Ok(())
}
