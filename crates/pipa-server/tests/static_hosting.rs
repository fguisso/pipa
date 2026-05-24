//! `/p/<uuid>/*` semantics: visibility gates, SPA fallback vs static 404,
//! security headers (CSP), and the password-cookie flow.

mod common;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pipa_adapters::hash_password;
use pipa_core::page::{Mode, NewPage, Visibility};

use crate::common::{spawn_test_server, TestServer};

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Seed a single-file page directly via the state. Returns the uuid.
async fn seed_page_with_index(
    server: &TestServer,
    uuid: &str,
    body: &str,
    mode: Mode,
    visibility: Visibility,
    password_plaintext: Option<&str>,
) {
    let pages_dir: PathBuf = server.data_root.path().join("pages");
    fs::create_dir_all(pages_dir.join(uuid)).expect("page dir");
    fs::write(pages_dir.join(uuid).join("index.html"), body).expect("index.html");

    let password_hash = password_plaintext
        .map(|p| hash_password(p).expect("hash"));

    server
        .state
        .repo
        .create_page(NewPage {
            uuid: uuid.into(),
            name: None,
            mode,
            visibility,
            password_hash,
            owner_kind: "local".into(),
            owner_id: "local".into(),
            size_bytes: body.len() as u64,
            file_count: 1,
            created_at: now_ts(),
            updated_at: now_ts(),
        })
        .await
        .expect("create page row");
}

#[tokio::test(flavor = "multi_thread")]
async fn public_page_serves_index_with_csp() {
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0001";
    let body = "<h1>hello world</h1>";
    seed_page_with_index(&server, uuid, body, Mode::Static, Visibility::Public, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200, "expected 200, got {resp:?}");
    assert!(
        resp.headers()
            .get("content-security-policy")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("")
            .contains("default-src"),
        "CSP must be present on page responses",
    );
    let text = resp.text().await.expect("text");
    assert_eq!(text, body);
}

#[tokio::test(flavor = "multi_thread")]
async fn spa_mode_falls_back_to_index_for_missing_path() {
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0002";
    seed_page_with_index(
        &server,
        uuid,
        "<h1>spa root</h1>",
        Mode::Spa,
        Visibility::Public,
        None,
    )
    .await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/p/{}/missing.html", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200, "SPA fallback must serve index");
    let text = resp.text().await.expect("text");
    assert!(text.contains("spa root"));
}

#[tokio::test(flavor = "multi_thread")]
async fn static_mode_returns_404_for_missing_path() {
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0003";
    seed_page_with_index(
        &server,
        uuid,
        "<h1>static root</h1>",
        Mode::Static,
        Visibility::Public,
        None,
    )
    .await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/p/{}/missing.html", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn private_page_returns_404() {
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0004";
    seed_page_with_index(
        &server,
        uuid,
        "<h1>secret</h1>",
        Mode::Static,
        Visibility::Private,
        None,
    )
    .await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(
        resp.status().as_u16(),
        404,
        "private pages must 404, not 401/403, to avoid leaking existence"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn password_page_gates_then_serves_with_cookie() {
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0005";
    seed_page_with_index(
        &server,
        uuid,
        "<h1>members only</h1>",
        Mode::Static,
        Visibility::Password,
        Some("hunter2"),
    )
    .await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");

    // No cookie → gate page (200, HTML form, NOT the secret).
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.expect("text");
    assert!(!body.contains("members only"), "gate must not leak content");
    assert!(
        body.to_lowercase().contains("password"),
        "should be the gate page"
    );

    // Submit the right password → 303 + Set-Cookie. Capture the cookie
    // header manually since the reqwest feature set doesn't include a jar.
    let resp = client
        .post(format!("{}/p/{}/__gate", server.base(), uuid))
        .form(&[("password", "hunter2"), ("next", "")])
        .send()
        .await
        .expect("submit gate");
    assert_eq!(
        resp.status().as_u16(),
        303,
        "successful submit must redirect (303)"
    );
    let raw_cookie = resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("Set-Cookie present on gate success")
        .to_str()
        .expect("ascii cookie")
        .to_string();
    // `Set-Cookie: gpages_p_<uuid>=val; Path=...` → grab the `name=value`.
    let cookie_kv = raw_cookie
        .split(';')
        .next()
        .expect("cookie has name=value")
        .to_string();

    // Replay GET with the cookie value attached.
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .header(reqwest::header::COOKIE, &cookie_kv)
        .send()
        .await
        .expect("send after cookie");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.expect("text");
    assert!(body.contains("members only"), "cookie should unlock content");

    // Wrong password re-renders the gate (200), no cookie issued.
    let resp = client
        .post(format!("{}/p/{}/__gate", server.base(), uuid))
        .form(&[("password", "WRONG"), ("next", "")])
        .send()
        .await
        .expect("bad submit");
    assert_eq!(resp.status().as_u16(), 200, "wrong password re-renders gate");
    assert!(
        resp.headers().get(reqwest::header::SET_COOKIE).is_none(),
        "wrong password must not Set-Cookie",
    );
}

