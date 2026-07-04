//! `pipa workspace …` — list/create workspaces, set the active one, manage
//! members and quotas. Workspace endpoints authorize by the caller's user
//! identity + role (not token scope), so any minted token works.

use anyhow::Result;

use crate::cli::{WorkspaceAction, WorkspaceArgs};
use crate::commands::client_with_access;
use crate::config;
use crate::output::{check, kv};

pub async fn run(args: WorkspaceArgs, json: bool) -> Result<()> {
    match args.action {
        WorkspaceAction::Ls => ls(json).await,
        WorkspaceAction::Use { id } => use_ws(id, json).await,
        WorkspaceAction::Unset => unset_ws(json).await,
        WorkspaceAction::Create { name } => create(name, json).await,
        WorkspaceAction::Show { id } => show(id, json).await,
        WorkspaceAction::MemberAdd {
            ws,
            username,
            role,
        } => member_add(ws, username, role, json).await,
        WorkspaceAction::MemberRole { ws, user_id, role } => {
            member_role(ws, user_id, role, json).await
        }
        WorkspaceAction::MemberRm { ws, user_id } => member_rm(ws, user_id, json).await,
        WorkspaceAction::Quota {
            ws,
            max_pages,
            max_bytes,
        } => quota(ws, max_pages, max_bytes, json).await,
    }
}

async fn ls(json: bool) -> Result<()> {
    let (client, _s, access) = client_with_access("read:*").await?;
    let rows = client.list_workspaces(&access).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    let active = config::load().active_workspace;
    if rows.is_empty() {
        println!("(no workspaces)");
        return Ok(());
    }
    for m in &rows {
        let marker = if active.as_deref() == Some(m.workspace.id.as_str()) {
            " *"
        } else {
            ""
        };
        println!(
            "  {:<28} {:<8} {}{}",
            m.workspace.id, m.role, m.workspace.name, marker
        );
    }
    println!("\n  (* = active; `pipa workspace use <id>` to change)");
    Ok(())
}

async fn use_ws(id: String, json: bool) -> Result<()> {
    let mut cfg = config::load();
    cfg.active_workspace = Some(id.clone());
    config::save(&cfg)?;
    if json {
        println!("{}", serde_json::json!({ "active_workspace": id }));
    } else {
        println!("{} active workspace set to {id}", check());
    }
    Ok(())
}

async fn unset_ws(json: bool) -> Result<()> {
    let mut cfg = config::load();
    cfg.active_workspace = None;
    config::save(&cfg)?;
    if json {
        println!("{}", serde_json::json!({ "active_workspace": null }));
    } else {
        println!("{} active workspace cleared (deploys use your personal workspace)", check());
    }
    Ok(())
}

async fn create(name: String, json: bool) -> Result<()> {
    let (client, _s, access) = client_with_access("read:*").await?;
    let ws = client.create_workspace(&access, &name).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&ws)?);
        return Ok(());
    }
    println!("{} created workspace", check());
    println!("{}", kv("id", &ws.id));
    println!("{}", kv("name", &ws.name));
    println!("\n  set it active with: pipa workspace use {}", ws.id);
    Ok(())
}

async fn show(id: Option<String>, json: bool) -> Result<()> {
    let id = id
        .or_else(|| config::load().active_workspace)
        .ok_or_else(|| anyhow::anyhow!("no workspace given and none active — pass an id or run `pipa workspace use <id>`"))?;
    let (client, _s, access) = client_with_access("read:*").await?;
    let detail = client.get_workspace(&access, &id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&detail)?);
        return Ok(());
    }
    println!("{}", kv("id", &detail.workspace.id));
    println!("{}", kv("name", &detail.workspace.name));
    println!("{}", kv("kind", &detail.workspace.kind));
    println!("{}", kv("your role", &detail.my_role));
    let quota = match (detail.workspace.max_pages, detail.workspace.max_bytes) {
        (None, None) => "unlimited".to_string(),
        (p, b) => format!(
            "pages={} bytes={}",
            p.map(|v| v.to_string()).unwrap_or_else(|| "∞".into()),
            b.map(|v| v.to_string()).unwrap_or_else(|| "∞".into()),
        ),
    };
    println!("{}", kv("quota", &quota));
    println!("members");
    for m in &detail.members {
        println!("  {:<20} {:<8} {}", m.username, m.role, m.user_id);
    }
    Ok(())
}

async fn member_add(ws: String, username: String, role: String, json: bool) -> Result<()> {
    let (client, _s, access) = client_with_access("read:*").await?;
    let members = client.add_member(&access, &ws, &username, &role).await?;
    report_members(&members, json, &format!("added {username} as {role}"))
}

async fn member_role(ws: String, user_id: String, role: String, json: bool) -> Result<()> {
    let (client, _s, access) = client_with_access("read:*").await?;
    let members = client.set_member_role(&access, &ws, &user_id, &role).await?;
    report_members(&members, json, &format!("{user_id} is now {role}"))
}

async fn member_rm(ws: String, user_id: String, json: bool) -> Result<()> {
    let (client, _s, access) = client_with_access("read:*").await?;
    let members = client.remove_member(&access, &ws, &user_id).await?;
    report_members(&members, json, &format!("removed {user_id}"))
}

async fn quota(ws: String, max_pages: Option<i64>, max_bytes: Option<i64>, json: bool) -> Result<()> {
    let (client, _s, access) = client_with_access("read:*").await?;
    let updated = client.set_quota(&access, &ws, max_pages, max_bytes).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&updated)?);
        return Ok(());
    }
    println!("{} quota updated for {}", check(), updated.id);
    Ok(())
}

fn report_members(members: &[pipa_sdk::MemberInfo], json: bool, msg: &str) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(members)?);
        return Ok(());
    }
    println!("{} {msg}", check());
    for m in members {
        println!("  {:<20} {:<8} {}", m.username, m.role, m.user_id);
    }
    Ok(())
}
