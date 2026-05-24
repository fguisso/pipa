//! `GET /health` shape, security headers, and the explicit "no Server header"
//! guarantee from SECURITY.md §2.

mod common;

use crate::common::spawn_test_server;

#[tokio::test(flavor = "multi_thread")]
async fn health_returns_200_no_body_with_hardening_headers() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/health", server.base()))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);

    // Required hardening headers.
    let h = resp.headers().clone();
    assert_eq!(
        h.get("x-content-type-options").and_then(|v| v.to_str().ok()),
        Some("nosniff"),
    );
    assert_eq!(
        h.get("referrer-policy").and_then(|v| v.to_str().ok()),
        Some("strict-origin-when-cross-origin"),
    );
    assert!(
        h.get("permissions-policy").is_some(),
        "permissions-policy must be set",
    );

    // Information-leak headers must NOT be present.
    assert!(h.get("server").is_none(), "Server header must be stripped");
    assert!(
        h.get("x-powered-by").is_none(),
        "X-Powered-By header must be stripped",
    );

    let body = resp.bytes().await.expect("body");
    assert!(body.is_empty(), "/health must have an empty body, got {body:?}");
}
