//! End-to-end deploy flow exercised purely over HTTP. We follow the same
//! happy path a fresh user would: claim the server via /setup, pair a CLI
//! device from the browser, mint a deploy token, upload a zip, fetch it back
//! from the public serving URL, flip visibility, and finally delete with
//! step-up.

mod common;

use reqwest::multipart::{Form, Part};
use serde_json::{Value, json};

use crate::common::{make_zip_with_index, mint_access, spawn_test_server};

#[tokio::test(flavor = "multi_thread")]
async fn full_round_trip() {
    let server = spawn_test_server().await;
    // Cookie store enabled so the /setup POST's gapes_owner cookie is replayed
    // on subsequent requests (the browser side of the flow).
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest client");
    let base = server.base();

    // ── 1. Browser creates the admin user (first-boot wizard) ────────────
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
        .expect("setup post");
    assert!(
        matches!(resp.status().as_u16(), 200 | 302 | 303),
        "setup expected 2xx/3xx, got {}",
        resp.status()
    );

    // ── 2. CLI begins pairing ────────────────────────────────────────────
    let resp = client
        .post(format!("{base}/api/auth/device-init"))
        .json(&json!({ "scope": "interactive" }))
        .send()
        .await
        .expect("device-init");
    assert_eq!(resp.status().as_u16(), 200, "device-init expected 200");
    let init: Value = resp.json().await.expect("init json");
    let device_code = init["device_code"].as_str().expect("device_code").to_string();
    let device_secret = init["device_secret"].as_str().expect("device_secret").to_string();

    // ── 3. Browser POSTs /cli to approve (owner cookie already set) ──────
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
    assert_eq!(resp.status().as_u16(), 200, "cli approval expected 200");

    // ── 4. CLI polls for Approved ────────────────────────────────────────
    let resp = client
        .post(format!("{base}/api/auth/device-poll"))
        .json(&json!({
            "device_code": device_code,
            "device_secret": device_secret,
        }))
        .send()
        .await
        .expect("device-poll");
    assert_eq!(resp.status().as_u16(), 200);
    let poll: Value = resp.json().await.expect("poll json");
    assert_eq!(poll["status"].as_str(), Some("approved"), "{poll:?}");
    let refresh = poll["refresh_token"].as_str().expect("refresh_token").to_string();
    let device_id = poll["device_id"].as_str().expect("device_id").to_string();

    // ── 5. Mint deploy:new ───────────────────────────────────────────────
    let resp = client
        .post(format!("{base}/api/auth/mint"))
        .json(&json!({
            "refresh": refresh,
            "scope": "deploy:new",
            "ttl_sec": 300,
        }))
        .send()
        .await
        .expect("mint");
    assert_eq!(resp.status().as_u16(), 200);
    let mint: Value = resp.json().await.expect("mint json");
    let access = mint["access"].as_str().expect("access").to_string();

    // ── 6. Deploy a zip (multipart) ──────────────────────────────────────
    let body_html = "<h1>hello from e2e</h1>";
    let zip = make_zip_with_index(body_html);
    let form = Form::new()
        .part(
            "archive",
            Part::bytes(zip)
                .file_name("archive.zip")
                .mime_str("application/zip")
                .expect("mime"),
        )
        .text("visibility", "public");
    let resp = client
        .post(format!("{base}/api/pages"))
        .bearer_auth(&access)
        .multipart(form)
        .send()
        .await
        .expect("deploy");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "deploy expected 200, got {} body {}",
        resp.status().as_u16(),
        resp.text().await.unwrap_or_default()
    );
    let deploy: Value = resp.json().await.expect("deploy json");
    let uuid = deploy["uuid"].as_str().expect("uuid").to_string();
    assert_eq!(deploy["visibility"].as_str(), Some("public"));
    assert!(deploy["url"].as_str().unwrap_or("").ends_with(&uuid));

    // ── 7. Public GET serves the index ───────────────────────────────────
    let resp = client
        .get(format!("{base}/p/{uuid}/"))
        .send()
        .await
        .expect("public get");
    assert_eq!(resp.status().as_u16(), 200);
    assert!(resp.text().await.expect("text").contains("hello from e2e"));

    // ── 8. Flip to private — requires admin:<uuid>, no step-up ───────────
    let admin = mint_access(&server.state, &device_id, &format!("admin:{uuid}"), 60);
    let resp = client
        .post(format!("{base}/api/pages/{uuid}/visibility"))
        .bearer_auth(&admin)
        .json(&json!({ "visibility": "private" }))
        .send()
        .await
        .expect("visibility");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "visibility flip expected 200, body {}",
        resp.text().await.unwrap_or_default()
    );

    // ── 9. /p/<uuid>/ now 404s ───────────────────────────────────────────
    let resp = client
        .get(format!("{base}/p/{uuid}/"))
        .send()
        .await
        .expect("post-private get");
    assert_eq!(resp.status().as_u16(), 404, "private page must 404");

    // ── 10. Mint destroy:<uuid> + step-up + DELETE ──────────────────────
    let destroy = mint_access(&server.state, &device_id, &format!("destroy:{uuid}"), 60);

    // Begin step-up and immediately confirm via the in-process store; this is
    // exactly what the /confirm/<code> POST does, just without rendering HTML.
    let stepup = server
        .state
        .auth
        .begin_step_up(&device_id, "page.delete", Some(&uuid), None)
        .await
        .expect("begin stepup");
    server
        .state
        .auth
        .confirm_step_up(&stepup.code)
        .await
        .expect("confirm stepup");

    let resp = client
        .delete(format!("{base}/api/pages/{uuid}"))
        .bearer_auth(&destroy)
        .header("X-Stepup-Code", &stepup.code)
        .send()
        .await
        .expect("delete");
    assert_eq!(
        resp.status().as_u16(),
        204,
        "delete expected 204, body {}",
        resp.text().await.unwrap_or_default()
    );

    // Trash directory should now hold a bundle for the deleted page.
    let trash = server.data_root.path().join("pages").join(".trash");
    let count = std::fs::read_dir(&trash).map(|rd| rd.count()).unwrap_or(0);
    assert!(
        count > 0,
        "trash must contain the deleted bundle, got {count} entries at {}",
        trash.display()
    );

    // Re-deploys after delete behave like a 404 — the row is gone.
    let resp = client
        .get(format!("{base}/p/{uuid}/"))
        .send()
        .await
        .expect("post-delete get");
    assert_eq!(resp.status().as_u16(), 404, "deleted page must 404");
}
