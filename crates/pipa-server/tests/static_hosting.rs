//! `/p/<uuid>/*` semantics: visibility gates, SPA fallback vs static 404,
//! security headers (CSP), and the password-cookie flow.

mod common;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pipa_adapters::hash_password;
use pipa_core::page::{Access, Csp, Mode, NewPage, Zone};

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
    access: Access,
    password_plaintext: Option<&str>,
) {
    seed_page_with_index_csp(server, uuid, body, mode, access, password_plaintext, Csp::Strict)
        .await;
}

#[allow(clippy::too_many_arguments)]
async fn seed_page_with_index_csp(
    server: &TestServer,
    uuid: &str,
    body: &str,
    mode: Mode,
    access: Access,
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
            access,
            zone: Zone::Public,
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
    seed_page_with_index(&server, uuid, body, Mode::Static, Access::Noauth, None).await;

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
        Access::Noauth,
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
        Access::Noauth,
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
async fn archived_page_returns_404() {
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0004";
    seed_page_with_index(
        &server,
        uuid,
        "<h1>secret</h1>",
        Mode::Static,
        Access::Noauth,
        None,
    )
    .await;
    // Archiving unpublishes the page — the role the legacy `private` value used
    // to play. It must 404 (not 401/403) to avoid leaking existence.
    server
        .state
        .repo
        .set_page_archived(uuid, true)
        .await
        .expect("archive");

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(
        resp.status().as_u16(),
        404,
        "archived pages must 404, not 401/403, to avoid leaking existence"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn gate_styles_are_served_locally_under_strict_csp() {
    // The gate page must NOT inline its CSS: `render_gate` emits the strict
    // `default-src 'self'` CSP, which would block an inline <style>. Instead it
    // links a same-origin stylesheet that the same policy allows.
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE000C";
    seed_page_with_index(
        &server,
        uuid,
        "<h1>members only</h1>",
        Mode::Static,
        Access::Password,
        Some("hunter2"),
    )
    .await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");

    // Gate page: strict CSP, links the local stylesheet, no inline <style>.
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
    let csp = resp
        .headers()
        .get("content-security-policy")
        .expect("gate must carry CSP")
        .to_str()
        .expect("ascii csp")
        .to_string();
    assert!(
        csp.contains("default-src 'self'"),
        "gate CSP should stay strict: {csp}"
    );
    let body = resp.text().await.expect("text");
    assert!(
        body.contains(r#"<link rel="stylesheet" href="/__gate.css""#),
        "gate must link the local stylesheet"
    );
    assert!(
        !body.contains("<style"),
        "gate must not inline CSS (blocked by strict CSP)"
    );

    // The stylesheet is served from our own origin as text/css → permitted by
    // `default-src 'self'`.
    let resp = client
        .get(format!("{}/__gate.css", server.base()))
        .send()
        .await
        .expect("send css");
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("text/css; charset=utf-8"),
        "stylesheet must be served as text/css (nosniff is set globally)"
    );
    let css = resp.text().await.expect("css body");
    assert!(css.contains(".card"), "stylesheet should carry the gate styles");
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
        Access::Password,
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
        Access::Noauth,
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
        Access::Noauth,
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
        Access::Noauth,
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

#[tokio::test(flavor = "multi_thread")]
async fn markdown_is_served_with_utf8_charset() {
    // Regression: text/* assets were served with the bare mime from mime_guess
    // (e.g. `text/markdown`), so browsers guessed the encoding and accented
    // bytes turned to mojibake. Every text/* response must declare utf-8.
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0009";
    seed_page_with_index(
        &server,
        uuid,
        "<h1>index</h1>",
        Mode::Static,
        Access::Noauth,
        None,
    )
    .await;

    // Drop a markdown sibling with accented (non-ASCII) content.
    let md_body = "# Relatório\n\nAção concluída — verificação OK.\n";
    let pages_dir: PathBuf = server.data_root.path().join("pages");
    fs::write(pages_dir.join(uuid).join("report.md"), md_body).expect("report.md");

    let resp = reqwest::Client::new()
        .get(format!("{}/p/{}/report.md", server.base(), uuid))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        ctype.starts_with("text/") && ctype.to_lowercase().contains("charset=utf-8"),
        "markdown must be served as text/* with charset=utf-8, got {ctype:?}",
    );
    // Bytes round-trip as UTF-8 (no mojibake).
    let body = resp.text().await.expect("text");
    assert!(body.contains("Relatório") && body.contains("Ação"));
}

#[tokio::test(flavor = "multi_thread")]
async fn password_page_unlocks_via_basic_auth() {
    // Non-interactive path: a headless caller (agent/curl) presents the page
    // password via HTTP Basic Auth and gets the content in one request — no
    // unlock form, no cookie.
    let server = spawn_test_server().await;
    let uuid = "01HXYZTEST00000000PAGE0010";
    seed_page_with_index(
        &server,
        uuid,
        "<h1>members only</h1>",
        Mode::Static,
        Access::Password,
        Some("hunter2"),
    )
    .await;

    let client = reqwest::Client::new();

    // Conventional form: username is the page uuid → 200 with the content.
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .basic_auth(uuid, Some("hunter2"))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.expect("text");
    assert!(
        body.contains("members only"),
        "uuid:password Basic Auth must unlock content"
    );

    // Bare form: empty username + right password is also accepted.
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .basic_auth("", Some("hunter2"))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
    assert!(resp.text().await.expect("text").contains("members only"));

    // Right password but a username naming a *different* page → gate.
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .basic_auth("01HXYZTEST00000000PAGE9999", Some("hunter2"))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.expect("text");
    assert!(
        !body.contains("members only") && body.to_lowercase().contains("password"),
        "mismatched username must fall back to the gate"
    );

    // Wrong password → gate, never the secret.
    let resp = client
        .get(format!("{}/p/{}/", server.base(), uuid))
        .basic_auth(uuid, Some("WRONG"))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.expect("text");
    assert!(
        !body.contains("members only") && body.to_lowercase().contains("password"),
        "wrong Basic Auth password must fall back to the gate"
    );
}
