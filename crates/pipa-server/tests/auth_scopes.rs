//! Scope-enforcement boundaries that mint and stepup-init must hold:
//!
//! 1. An automation-scope refresh token cannot mint `destroy:<uuid>` (mint
//!    rejects with 403). It CAN mint `deploy:new` (200).
//! 2. An automation-scope bearer cannot start a step-up flow (stepup-init
//!    rejects with 403).
//!
//! See `phase-1-auth.md` §Scopes for the policy.

mod common;

use pipa_core::device::Scope;
use serde_json::{Value, json};

use crate::common::{mint_access, spawn_test_server};

#[tokio::test(flavor = "multi_thread")]
async fn automation_refresh_cannot_mint_destroy_but_can_deploy() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::new();
    let base = server.base();

    // Build an automation device + refresh directly via the store (faster
    // than running the pairing flow and equivalent for the scope check).
    let device = server
        .state
        .auth
        .create_device("ci-runner", Scope::Automation)
        .await
        .expect("create device");
    let (_t, refresh) = server
        .state
        .auth
        .issue_refresh(&device.id, Scope::Automation, 3600)
        .await
        .expect("issue refresh");

    // destroy:<uuid> must be rejected with 403.
    let resp = client
        .post(format!("{base}/api/auth/mint"))
        .json(&json!({
            "refresh": refresh,
            "scope": "destroy:01HXYZ_TEST",
            "ttl_sec": 60,
        }))
        .send()
        .await
        .expect("mint destroy");
    assert_eq!(
        resp.status().as_u16(),
        403,
        "automation must not mint destroy"
    );
    let err: Value = resp.json().await.expect("err json");
    assert_eq!(err["error"].as_str(), Some("automation_cannot_escalate"));

    // The mint endpoint rotates refresh on failure? No — only on success.
    // Confirm by minting deploy:new with the SAME refresh.
    let resp = client
        .post(format!("{base}/api/auth/mint"))
        .json(&json!({
            "refresh": refresh,
            "scope": "deploy:new",
            "ttl_sec": 60,
        }))
        .send()
        .await
        .expect("mint deploy");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "automation must be allowed to deploy:new, body {}",
        resp.text().await.unwrap_or_default()
    );

    // Similarly admin:* and manage:* must be rejected.
    for forbidden in ["admin:01HXYZ_TEST", "manage:devices"] {
        // Need a fresh refresh because the previous successful mint rotated
        // the one we just used. Issue a new one.
        let (_t, fresh) = server
            .state
            .auth
            .issue_refresh(&device.id, Scope::Automation, 3600)
            .await
            .expect("issue refresh");
        let resp = client
            .post(format!("{base}/api/auth/mint"))
            .json(&json!({
                "refresh": fresh,
                "scope": forbidden,
                "ttl_sec": 60,
            }))
            .send()
            .await
            .expect("mint forbidden");
        assert_eq!(
            resp.status().as_u16(),
            403,
            "automation must not mint {forbidden}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn automation_bearer_cannot_stepup_init() {
    let server = spawn_test_server().await;
    let client = reqwest::Client::new();
    let base = server.base();

    let device = server
        .state
        .auth
        .create_device("ci-runner", Scope::Automation)
        .await
        .expect("create device");

    // Forge an access token for the automation device. The mint endpoint
    // would refuse to grant admin/manage but the stepup-init guard is its
    // own defense layer — even an access token claiming admin: scope must
    // fail because the device.scope itself is Automation. We give it
    // `deploy:new` here — any non-empty valid scope works for the test
    // because stepup-init checks the *device* not the access scope.
    let bearer = mint_access(&server.state, &device.id, "deploy:new", 60);

    let resp = client
        .post(format!("{base}/api/auth/stepup-init"))
        .bearer_auth(&bearer)
        .json(&json!({ "operation": "page.delete", "target": "any" }))
        .send()
        .await
        .expect("stepup-init");
    assert_eq!(
        resp.status().as_u16(),
        403,
        "automation must be forbidden from step-up"
    );
    let body: Value = resp.json().await.expect("err json");
    assert_eq!(body["error"].as_str(), Some("automation_scope_cannot_stepup"));
}
