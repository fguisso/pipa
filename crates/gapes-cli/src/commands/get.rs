//! `gapes get <uuid>` — pretty-print page metadata (or JSON with --json).

use anyhow::Result;

use crate::cli::GetArgs;
use crate::commands::client_with_access;
use crate::output::{fmt_ts, human_bytes, kv};

pub async fn run(args: GetArgs, json: bool) -> Result<()> {
    let scope = format!("read:{}", args.uuid);
    let (client, _server, access) = client_with_access(&scope).await?;
    let page = client.get_page(&access, &args.uuid).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&page)?);
        return Ok(());
    }

    println!("{}", kv("uuid", &page.uuid));
    println!("{}", kv("name", page.name.as_deref().unwrap_or("—")));
    println!("{}", kv("mode", &page.mode));
    println!("{}", kv("access", &page.access));
    println!("{}", kv("zone", &page.zone));
    println!("{}", kv("size", &human_bytes(page.size_bytes)));
    println!("{}", kv("files", &page.file_count.to_string()));
    println!("{}", kv("comments", &format!("enabled={} approval={}", page.comments_enabled, page.comments_require_approval)));
    println!("{}", kv("created", &fmt_ts(page.created_at)));
    println!("{}", kv("updated", &fmt_ts(page.updated_at)));
    Ok(())
}
