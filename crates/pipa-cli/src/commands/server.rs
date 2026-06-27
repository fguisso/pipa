//! `pipa server` — show the target server and the optional features it
//! enforces (via authenticated `GET /api/meta`). Lets you know up front
//! whether feature-dependent flags like `--zone` will actually be honored.

use anyhow::Result;

use crate::commands::client_with_access;
use crate::output::{check, kv};

pub async fn run(json: bool) -> Result<()> {
    let (client, server, access) = client_with_access("read:*").await?;
    let features = client
        .meta(&access)
        .await
        .map(|m| m.features)
        .unwrap_or_default();

    if json {
        println!(
            "{}",
            serde_json::json!({ "server": server, "features": features })
        );
        return Ok(());
    }

    println!("{} {}", check(), server);
    let feat = if features.is_empty() {
        "(none)".to_string()
    } else {
        features.join(", ")
    };
    println!("{}", kv("features", &feat));
    Ok(())
}
