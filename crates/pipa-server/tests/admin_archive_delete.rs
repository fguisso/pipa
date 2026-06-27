//! Admin-only archive + delete endpoints driven through the cookie session.
//!
//! Mirrors the deploy_e2e setup: create admin via /setup (gets owner cookie),
//! pair a CLI device through /cli, mint a deploy token, push a zip. Then
//! exercise the two admin endpoints — archive flips visibility off without
//! touching disk; delete blows the bundle away.

mod common;

use reqwest::multipart::{Form, Part};
use serde_json::{Value, json};

use crate::common::{make_zip_with_index, spawn_test_server};

#[tokio::test(flavor = "multi_thread")]
async fn archive_then_unarchive_then_delete() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest client");
    let base = server.base();

    // ── 1. Create admin ──────────────────────────────────────────────────
    let resp = client
        .post(format!("{base}/setup"))
        .form(&[
            ("username", "ci-admin"),
            ("password", "test-password-1"),
            ("password_confirm", "test-password-1"),
            ("next", "/"),
        ])
        .send()
        .await
        .expect("setup");
    assert!(
        matches!(resp.status().as_u16(), 200 | 302 | 303),
        "setup status: {}",
        resp.status()
    );

    // ── 2. Pair a CLI device + grab a deploy token ───────────────────────
    let init: Value = client
        .post(format!("{base}/api/auth/device-init"))
        .json(&json!({ "scope": "interactive" }))
        .send()
        .await
        .expect("device-init")
        .json()
        .await
        .expect("init json");
    let device_code = init["device_code"].as_str().unwrap().to_string();
    let device_secret = init["device_secret"].as_str().unwrap().to_string();

    let resp = client
        .post(format!("{base}/cli"))
        .form(&[
            ("device_code", device_code.as_str()),
            ("label", "Test CI"),
            ("scope", "interactive"),
        ])
        .send()
        .await
        .expect("cli post");
    assert_eq!(resp.status().as_u16(), 200);

    let poll: Value = client
        .post(format!("{base}/api/auth/device-poll"))
        .json(&json!({ "device_code": device_code, "device_secret": device_secret }))
        .send()
        .await
        .expect("device-poll")
        .json()
        .await
        .expect("poll json");
    let refresh = poll["refresh_token"].as_str().unwrap().to_string();

    let mint: Value = client
        .post(format!("{base}/api/auth/mint"))
        .json(&json!({ "refresh": refresh, "scope": "deploy:new", "ttl_sec": 300 }))
        .send()
        .await
        .expect("mint")
        .json()
        .await
        .expect("mint json");
    let access = mint["access"].as_str().unwrap().to_string();

    // ── 3. Deploy a tiny public page ─────────────────────────────────────
    let zip = make_zip_with_index("<h1>before archive</h1>");
    let form = Form::new()
        .part(
            "archive",
            Part::bytes(zip)
                .file_name("a.zip")
                .mime_str("application/zip")
                .expect("mime"),
        )
        .text("access", "noauth");
    let deploy: Value = client
        .post(format!("{base}/api/pages"))
        .bearer_auth(&access)
        .multipart(form)
        .send()
        .await
        .expect("deploy")
        .json()
        .await
        .expect("deploy json");
    let uuid = deploy["uuid"].as_str().unwrap().to_string();

    let bundle = server.data_root.path().join("pages").join(&uuid);
    assert!(bundle.exists(), "bundle dir must exist after deploy");

    // Public GET should hit the page.
    let resp = client
        .get(format!("{base}/p/{uuid}/"))
        .send()
        .await
        .expect("public get");
    assert_eq!(resp.status().as_u16(), 200);

    // ── 4. Archive → public 404, bundle preserved ────────────────────────
    let resp = client
        .post(format!("{base}/api/admin/pages/{uuid}/archive"))
        .json(&json!({ "archived": true }))
        .send()
        .await
        .expect("archive");
    assert_eq!(resp.status().as_u16(), 200, "archive expected 200");

    let resp = client
        .get(format!("{base}/p/{uuid}/"))
        .send()
        .await
        .expect("public get after archive");
    assert_eq!(resp.status().as_u16(), 404, "archived page must 404");
    assert!(bundle.exists(), "archive must NOT remove files from disk");

    // ── 5. Unarchive → public 200 again ──────────────────────────────────
    let resp = client
        .post(format!("{base}/api/admin/pages/{uuid}/archive"))
        .json(&json!({ "archived": false }))
        .send()
        .await
        .expect("unarchive");
    assert_eq!(resp.status().as_u16(), 200, "unarchive expected 200");
    let resp = client
        .get(format!("{base}/p/{uuid}/"))
        .send()
        .await
        .expect("public get after unarchive");
    assert_eq!(resp.status().as_u16(), 200, "unarchived page must serve");

    // ── 6. Admin delete → 204, bundle gone, row gone ─────────────────────
    let resp = client
        .delete(format!("{base}/api/admin/pages/{uuid}"))
        .send()
        .await
        .expect("admin delete");
    assert_eq!(resp.status().as_u16(), 204, "admin delete expected 204");
    let resp = client
        .get(format!("{base}/p/{uuid}/"))
        .send()
        .await
        .expect("public get after delete");
    assert_eq!(resp.status().as_u16(), 404, "deleted page must 404");
    assert!(!bundle.exists(), "delete must remove the bundle directory");
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_endpoints_require_owner_cookie() {
    let server = spawn_test_server().await;
    // Fresh client with NO cookie store — should be rejected by AdminSession.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest client");
    let base = server.base();

    // Archive without cookie — AdminSession redirects to /setup (no admin
    // yet) or /admin/login (admin exists). Either way it's a 3xx, not a
    // successful 2xx — the endpoint must not run the mutation.
    let resp = client
        .post(format!("{base}/api/admin/pages/nonexistent/archive"))
        .json(&json!({ "archived": true }))
        .send()
        .await
        .expect("archive");
    assert!(
        resp.status().is_redirection() || resp.status().as_u16() == 401,
        "unauth archive must 3xx/401, got {}",
        resp.status()
    );

    let resp = client
        .delete(format!("{base}/api/admin/pages/nonexistent"))
        .send()
        .await
        .expect("delete");
    assert!(
        resp.status().is_redirection() || resp.status().as_u16() == 401,
        "unauth delete must 3xx/401, got {}",
        resp.status()
    );
}
