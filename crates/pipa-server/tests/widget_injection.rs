//! Server-side widget injection: when comments_enabled is set, every HTML
//! response for the page gets a <script src=/api/comments/widget.js> tag
//! spliced in before </body>. The stored bundle stays untouched — toggling
//! the flag back off makes the very next request serve the original bytes.

mod common;

use reqwest::multipart::{Form, Part};
use serde_json::{Value, json};

use crate::common::{make_zip_with_index, spawn_test_server};

const WIDGET_SRC: &str = "/api/comments/widget.js";

#[tokio::test(flavor = "multi_thread")]
async fn html_response_includes_widget_when_enabled() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest client");
    let base = server.base();

    // ── claim admin ──────────────────────────────────────────────────────
    let resp = client
        .post(format!("{base}/setup"))
        .form(&[
            ("username", "ci-admin"),
            ("password", "test-password-1"),
            ("password_confirm", "test-password-1"),
        ])
        .send()
        .await
        .expect("setup");
    assert!(matches!(resp.status().as_u16(), 200 | 302 | 303));

    // ── pair + mint deploy token ────────────────────────────────────────
    let init: Value = client
        .post(format!("{base}/api/auth/device-init"))
        .json(&json!({ "scope": "interactive" }))
        .send()
        .await
        .expect("init")
        .json()
        .await
        .unwrap();
    let device_code = init["device_code"].as_str().unwrap().to_string();
    let device_secret = init["device_secret"].as_str().unwrap().to_string();

    client
        .post(format!("{base}/cli"))
        .form(&[
            ("device_code", device_code.as_str()),
            ("label", "Test"),
            ("scope", "interactive"),
        ])
        .send()
        .await
        .expect("cli post");

    let poll: Value = client
        .post(format!("{base}/api/auth/device-poll"))
        .json(&json!({ "device_code": device_code, "device_secret": device_secret }))
        .send()
        .await
        .expect("poll")
        .json()
        .await
        .unwrap();
    let refresh = poll["refresh_token"].as_str().unwrap().to_string();

    let mint: Value = client
        .post(format!("{base}/api/auth/mint"))
        .json(&json!({ "refresh": refresh, "scope": "deploy:new", "ttl_sec": 300 }))
        .send()
        .await
        .expect("mint")
        .json()
        .await
        .unwrap();
    let access = mint["access"].as_str().unwrap().to_string();

    // ── deploy a public HTML page ────────────────────────────────────────
    let original_html = "<!doctype html><html><body><h1>hello</h1></body></html>";
    let zip = make_zip_with_index(original_html);
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
        .unwrap();
    let uuid = deploy["uuid"].as_str().unwrap().to_string();

    // ── comments OFF → original bytes ────────────────────────────────────
    let resp = client.get(format!("{base}/p/{uuid}/")).send().await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.unwrap();
    assert!(
        !body.contains(WIDGET_SRC),
        "widget must NOT be injected when comments disabled, body: {body}"
    );

    // ── enable comments via repo (the API call goes through the admin
    //    cookie path; for the unit-style test we flip the flag directly to
    //    keep the assertion focused on the injection behavior). ──────────
    server
        .state
        .repo
        .enable_comments(&uuid, true, false)
        .await
        .expect("enable comments");

    // ── comments ON → widget script appears before </body> ──────────────
    let resp = client.get(format!("{base}/p/{uuid}/")).send().await.unwrap();
    let body = resp.text().await.unwrap();
    assert!(
        body.contains(WIDGET_SRC),
        "widget MUST be injected when comments enabled, body: {body}"
    );
    let script_pos = body.find(WIDGET_SRC).unwrap();
    let body_close = body.to_ascii_lowercase().rfind("</body>").unwrap();
    assert!(
        script_pos < body_close,
        "script must come before </body> (script_pos={script_pos}, body_close={body_close})"
    );
    assert!(
        body.contains(&format!("data-page=\"{uuid}\"")),
        "data-page attribute missing from injected tag"
    );

    // Confirm the *stored* file on disk is unchanged — reversibility hinges
    // on this. The bundle path mirrors how DiskStorage lays out pages.
    let on_disk = server
        .data_root
        .path()
        .join("pages")
        .join(&uuid)
        .join("index.html");
    let disk_bytes = std::fs::read_to_string(&on_disk).expect("read on-disk html");
    assert_eq!(
        disk_bytes, original_html,
        "stored file must not be modified by injection"
    );

    // ── flip back off → next request is byte-for-byte original ──────────
    server
        .state
        .repo
        .enable_comments(&uuid, false, false)
        .await
        .expect("disable comments");
    let body = client
        .get(format!("{base}/p/{uuid}/"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !body.contains(WIDGET_SRC),
        "disabling comments must remove the widget from subsequent responses"
    );
    assert_eq!(
        body, original_html,
        "disabled response must match the stored bytes exactly"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn widget_css_served() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::new();
    let base = server.base();

    let resp = client
        .get(format!("{base}/api/comments/widget.css"))
        .send()
        .await
        .expect("widget.css");
    assert_eq!(resp.status().as_u16(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.starts_with("text/css"),
        "content-type for widget.css must be text/css, got {ct}"
    );
}
