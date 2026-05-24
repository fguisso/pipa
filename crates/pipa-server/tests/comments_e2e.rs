//! Comments — public POST + sanitize + rate limiting + moderation.
//!
//! We pre-seed a page directly through the repo (no HTTP deploy required —
//! the deploy path is covered in `deploy_e2e.rs`) and then drive the
//! comments surface end-to-end.

mod common;

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use pipa_core::page::{Mode, NewPage, Visibility};
use serde_json::{Value, json};

use crate::common::{mint_access, spawn_test_server, TestServer};

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn seed_public_page(server: &TestServer, uuid: &str) {
    let pages_dir = server.data_root.path().join("pages").join(uuid);
    fs::create_dir_all(&pages_dir).expect("page dir");
    fs::write(pages_dir.join("index.html"), "<h1>page</h1>").expect("index");
    server
        .state
        .repo
        .create_page(NewPage {
            uuid: uuid.into(),
            name: None,
            mode: Mode::Static,
            visibility: Visibility::Public,
            password_hash: None,
            owner_kind: "local".into(),
            owner_id: "local".into(),
            size_bytes: 32,
            file_count: 1,
            created_at: now_ts(),
            updated_at: now_ts(),
        })
        .await
        .expect("create page");
}

#[tokio::test(flavor = "multi_thread")]
async fn comments_disabled_returns_404() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::new();
    let base = server.base();
    let uuid = "01HXYZTEST00000000COMMENTS00";
    // Seed a page with comments_enabled=0 (the default).
    seed_public_page(&server, uuid).await;

    // GET → 404 (never leak existence).
    let resp = client
        .get(format!("{base}/api/pages/{uuid}/comments"))
        .send()
        .await
        .expect("get");
    assert_eq!(resp.status().as_u16(), 404);

    // POST → 404 as well.
    let resp = client
        .post(format!("{base}/api/pages/{uuid}/comments"))
        .json(&json!({ "author": "x", "body": "y" }))
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn comments_sanitize_rate_limit_approval_and_moderation() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::new();
    let base = server.base();
    let uuid = "01HXYZTEST00000000COMMENTS01";

    seed_public_page(&server, uuid).await;

    let admin = mint_access(&server.state, "test-device", &format!("admin:{uuid}"), 60);

    // ── Enable comments (no approval required) via the config endpoint ───
    let resp = client
        .post(format!("{base}/api/pages/{uuid}/comments-config"))
        .bearer_auth(&admin)
        .json(&json!({ "enabled": true, "require_approval": false }))
        .send()
        .await
        .expect("comments-config");
    assert_eq!(resp.status().as_u16(), 200);

    // ── POST a comment with HTML in body → sanitized response ────────────
    let resp = client
        .post(format!("{base}/api/pages/{uuid}/comments"))
        .json(&json!({
            "author": "alice",
            "body": "hello **world** <script>alert(1)</script>",
        }))
        .send()
        .await
        .expect("post comment");
    assert_eq!(resp.status().as_u16(), 200);
    let body: Value = resp.json().await.expect("comment json");
    let html = body["html"].as_str().expect("html");
    assert!(!html.contains("<script"), "<script> must be stripped");
    assert!(html.contains("<strong>world</strong>"), "markdown rendered");
    assert_eq!(body["status"].as_str(), Some("visible"));

    // Public GET must also be sanitized.
    let resp = client
        .get(format!("{base}/api/pages/{uuid}/comments"))
        .send()
        .await
        .expect("list comments");
    assert_eq!(resp.status().as_u16(), 200);
    let list: Value = resp.json().await.expect("list json");
    let first = &list["comments"][0];
    let public_html = first["html"].as_str().expect("public html");
    assert!(!public_html.contains("<script"));
    assert!(public_html.contains("<strong>world</strong>"));

    // ── Rate limit: 11th POST in <60s returns 429 ────────────────────────
    // We've already submitted 1; 9 more should succeed; the 11th must 429.
    for i in 0..9 {
        let resp = client
            .post(format!("{base}/api/pages/{uuid}/comments"))
            .json(&json!({ "author": "alice", "body": format!("burst {i}") }))
            .send()
            .await
            .expect("burst post");
        assert_eq!(
            resp.status().as_u16(),
            200,
            "burst {i} should pass; body {}",
            resp.text().await.unwrap_or_default()
        );
    }
    let resp = client
        .post(format!("{base}/api/pages/{uuid}/comments"))
        .json(&json!({ "author": "alice", "body": "one too many" }))
        .send()
        .await
        .expect("limit hit");
    assert_eq!(resp.status().as_u16(), 429, "11th should rate-limit");
    assert!(
        resp.headers().get("retry-after").is_some(),
        "Retry-After header should be present on 429"
    );

    // ── Flip require_approval=true; next POST is pending ─────────────────
    // Use a fresh page UUID so the rate limiter (keyed by (ip_hash, page))
    // doesn't fight us.
    let uuid2 = "01HXYZTEST00000000COMMENTS02";
    seed_public_page(&server, uuid2).await;
    let resp = client
        .post(format!("{base}/api/pages/{uuid2}/comments-config"))
        .bearer_auth(mint_access(&server.state, "test-device", &format!("admin:{uuid2}"), 60))
        .json(&json!({ "enabled": true, "require_approval": true }))
        .send()
        .await
        .expect("config approval");
    assert_eq!(resp.status().as_u16(), 200);

    let resp = client
        .post(format!("{base}/api/pages/{uuid2}/comments"))
        .json(&json!({ "author": "bob", "body": "moderate me" }))
        .send()
        .await
        .expect("pending post");
    assert_eq!(resp.status().as_u16(), 200);
    let pending: Value = resp.json().await.expect("pending json");
    assert_eq!(pending["status"].as_str(), Some("pending"));
    // Pending comments do NOT echo the body on submit either.
    assert!(pending["html"].as_str().unwrap_or("").is_empty());
    let pending_id = pending["id"].as_str().expect("id").to_string();

    // Pending comments are hidden from the public list.
    let resp = client
        .get(format!("{base}/api/pages/{uuid2}/comments"))
        .send()
        .await
        .expect("list pending");
    let list: Value = resp.json().await.expect("list json");
    assert_eq!(list["comments"].as_array().expect("arr").len(), 0);

    // Owner approves via PATCH.
    let admin2 = mint_access(&server.state, "test-device", &format!("admin:{uuid2}"), 60);
    let resp = client
        .patch(format!("{base}/api/comments/{pending_id}"))
        .bearer_auth(&admin2)
        .json(&json!({ "status": "visible" }))
        .send()
        .await
        .expect("patch");
    assert_eq!(resp.status().as_u16(), 200);

    // Now public sees it.
    let resp = client
        .get(format!("{base}/api/pages/{uuid2}/comments"))
        .send()
        .await
        .expect("list after approve");
    let list: Value = resp.json().await.expect("list json");
    let comments = list["comments"].as_array().expect("arr");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["id"].as_str(), Some(pending_id.as_str()));

    // DELETE → 204 and removed from the public list.
    let resp = client
        .delete(format!("{base}/api/comments/{pending_id}"))
        .bearer_auth(&admin2)
        .send()
        .await
        .expect("delete");
    assert_eq!(resp.status().as_u16(), 204);
    let resp = client
        .get(format!("{base}/api/pages/{uuid2}/comments"))
        .send()
        .await
        .expect("list after delete");
    let list: Value = resp.json().await.expect("list json");
    assert_eq!(list["comments"].as_array().expect("arr").len(), 0);
}
