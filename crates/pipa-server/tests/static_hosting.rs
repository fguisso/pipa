//! `/p/<uuid>/*` semantics: visibility gates, SPA fallback vs static 404,
//! security headers (CSP), and the password-cookie flow.

mod common;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pipa_adapters::hash_password;
use pipa_core::page::{Csp, Mode, NewPage, Visibility};

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
    seed_page_with_index_csp(server, uuid, body, mode, visibility, password_plaintext, Csp::Strict)
        .await;
}

#[allow(clippy::too_many_arguments)]
async fn seed_page_with_index_csp(
    server: &TestServer,
    uuid: &str,
    body: &str,
    mode: Mode,
    visibility: Visibility,
    password_plaintext: Option<&str>,
    csp: Csp,
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
            csp,
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

#[tokio::test(flavor = "multi_thread")]
async fn redirect_no_trailing_slash() {
    // GET /p/<uuid> (no trailing slash) must 308 → /p/<uuid>/, and preserve
    // any query string. Without this the browser resolves relative URLs in
    // index.html against the wrong parent and the CSS/JS 404s. See Bug A in
    // the change log.
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0006";
    seed_page_with_index(
        &server,
        uuid,
        "<h1>hello</h1>",
        Mode::Static,
        Visibility::Public,
        None,
    )
    .await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");

    // No query.
    let resp = client
        .get(format!("{}/p/{}", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 308, "expected 308 permanent redirect");
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some(format!("/p/{uuid}/").as_str()),
    );

    // Query preserved.
    let resp = client
        .get(format!("{}/p/{}?ref=tweet&utm=x", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 308);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some(format!("/p/{uuid}/?ref=tweet&utm=x").as_str()),
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn csp_off_omits_header() {
    // A page with csp=off must not have the platform's CSP header on its
    // responses — the page is expected to declare its own via <meta>.
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0007";
    seed_page_with_index_csp(
        &server,
        uuid,
        "<h1>csp off</h1>",
        Mode::Static,
        Visibility::Public,
        None,
        Csp::Off,
    )
    .await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/p/{}/index.html", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
    assert!(
        resp.headers().get("content-security-policy").is_none(),
        "csp=off must omit the content-security-policy header, got {:?}",
        resp.headers().get("content-security-policy"),
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn csp_strict_still_emits_header() {
    // Sibling assertion for the above — the default (strict) still emits the
    // platform CSP header.
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0008";
    seed_page_with_index_csp(
        &server,
        uuid,
        "<h1>csp strict</h1>",
        Mode::Static,
        Visibility::Public,
        None,
        Csp::Strict,
    )
    .await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/p/{}/index.html", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
    assert!(
        resp.headers()
            .get("content-security-policy")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .contains("default-src"),
        "csp=strict must emit the locked-down policy",
    );
}
