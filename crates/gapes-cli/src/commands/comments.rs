//! `gapes comments …` — thin wrapper over the SDK's comments calls. The
//! server-side endpoints are stubs in M0-M4; M5 fills them in. We emit a
//! friendly note when we hit a 501.

use anyhow::Result;
use gapes_sdk::{CommentsConfig, SdkError};
use tabled::settings::Style;
use tabled::{Table, Tabled};

use crate::cli::{CommentsAction, CommentsArgs};
use crate::commands::client_with_access;
use crate::output::{check, dim, fmt_ts};

#[derive(Tabled)]
struct Row {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "AUTHOR")]
    author: String,
    #[tabled(rename = "TS")]
    ts: String,
    #[tabled(rename = "ANCHOR")]
    anchor: String,
    #[tabled(rename = "PREVIEW")]
    preview: String,
}

pub async fn run(args: CommentsArgs) -> Result<()> {
    match args.action {
        CommentsAction::Enable { uuid } => {
            let (client, _, access) = client_with_access(&format!("admin:{uuid}")).await?;
            let cfg = CommentsConfig {
                enabled: true,
                require_approval: false,
            };
            match client.comments_set_config(&access, &uuid, &cfg).await {
                Ok(()) => println!("{} comments enabled on {uuid}", check()),
                Err(e) => note_stub_or_bubble(e)?,
            }
        }
        CommentsAction::Disable { uuid } => {
            let (client, _, access) = client_with_access(&format!("admin:{uuid}")).await?;
            let cfg = CommentsConfig {
                enabled: false,
                require_approval: false,
            };
            match client.comments_set_config(&access, &uuid, &cfg).await {
                Ok(()) => println!("{} comments disabled on {uuid}", check()),
                Err(e) => note_stub_or_bubble(e)?,
            }
        }
        CommentsAction::RequireApproval { uuid, on, off } => {
            let want_on = on || !off; // default to "on"
            let (client, _, access) = client_with_access(&format!("admin:{uuid}")).await?;
            let cfg = CommentsConfig {
                enabled: true,
                require_approval: want_on,
            };
            match client.comments_set_config(&access, &uuid, &cfg).await {
                Ok(()) => println!(
                    "{} comments on {uuid}: require_approval = {want_on}",
                    check()
                ),
                Err(e) => note_stub_or_bubble(e)?,
            }
        }
        CommentsAction::Ls { uuid, status } => {
            let (client, _, access) = client_with_access(&format!("read:{uuid}")).await?;
            let list = match client.comments_moderation_list(&access, &uuid).await {
                Ok(l) => l,
                Err(e) => return note_stub_or_bubble(e),
            };
            let rows: Vec<Row> = list
                .comments
                .iter()
                .filter(|c| status.as_deref().map(|s| s == c.status).unwrap_or(true))
                .map(|c| Row {
                    id: c.id.clone(),
                    status: c.status.clone(),
                    author: c.author.clone(),
                    ts: fmt_ts(c.ts),
                    anchor: preview(&c.anchor_text, 20),
                    preview: preview(&c.body_md, 40),
                })
                .collect();
            if rows.is_empty() {
                println!("{}", dim("(no comments)"));
            } else {
                let mut t = Table::new(rows);
                t.with(Style::modern_rounded());
                println!("{t}");
            }
        }
        CommentsAction::Show { id: _ } => {
            // M5 will likely add a `GET /api/comments/<id>` route. For now, no
            // single-fetch endpoint exists; surface that fact rather than
            // silently failing.
            println!(
                "{}",
                dim("single-comment fetch endpoint not implemented yet (M5)")
            );
        }
        CommentsAction::Approve { id } => {
            let (client, _, access) = client_with_access("manage:devices").await?;
            match client.comments_set_status(&access, &id, "visible").await {
                Ok(()) => println!("{} approved {id}", check()),
                Err(e) => note_stub_or_bubble(e)?,
            }
        }
        CommentsAction::Hide { id } => {
            let (client, _, access) = client_with_access("manage:devices").await?;
            match client.comments_set_status(&access, &id, "hidden").await {
                Ok(()) => println!("{} hidden {id}", check()),
                Err(e) => note_stub_or_bubble(e)?,
            }
        }
        CommentsAction::Rm { id } => {
            let (client, _, access) = client_with_access("manage:devices").await?;
            match client.comments_delete(&access, &id).await {
                Ok(()) => println!("{} deleted {id}", check()),
                Err(e) => note_stub_or_bubble(e)?,
            }
        }
    }
    Ok(())
}

fn note_stub_or_bubble(e: SdkError) -> Result<()> {
    // M5 is the comments milestone — until the server lands real routes,
    // calls return either 501 (stubs) or 404 (route not registered yet).
    // Treat both as a friendly "coming soon" rather than bubbling.
    if matches!(e.status(), Some(501) | Some(404)) {
        println!(
            "{}",
            dim("comments endpoints are not implemented on this server yet (M5)")
        );
        Ok(())
    } else {
        Err(e.into())
    }
}

fn preview(s: &str, n: usize) -> String {
    let one_line = s.replace('\n', " ").trim().to_string();
    if one_line.chars().count() <= n {
        one_line
    } else {
        let mut t: String = one_line.chars().take(n - 1).collect();
        t.push('…');
        t
    }
}
