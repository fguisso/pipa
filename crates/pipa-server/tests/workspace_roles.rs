//! Phase 4 workspace role enforcement + quota + transfer, exercised over the
//! real HTTP API. A `viewer` can read but not change a page; an `editor` can;
//! a non-member is rejected; a workspace quota blocks over-limit transfers; and
//! a transfer re-homes ownership.

mod common;

use pipa_core::device::{Device, Scope};
use pipa_core::page::{Access, Csp, Mode, NewPage, Zone};
use pipa_core::ports::AuthStore;
use pipa_core::user::{NewUser, User};
use pipa_core::workspace::{NewWorkspace, WorkspaceKind, WorkspaceRole};
use serde_json::{Value, json};

use crate::common::{mint_access, spawn_test_server};

fn ws_page(uuid: &str, ws_id: &str) -> NewPage {
    NewPage {
        uuid: uuid.to_string(),
        name: Some(uuid.to_string()),
        mode: Mode::Spa,
        access: Access::Noauth,
        zone: Zone::Public,
        password_hash: None,
        owner_kind: "workspace".to_string(),
        owner_id: ws_id.to_string(),
        size_bytes: 10,
        file_count: 1,
        csp: Csp::Strict,
        created_at: 1000,
        updated_at: 1000,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn workspace_roles_quota_and_transfer() {
    let server = spawn_test_server().await;
    let http = reqwest::Client::new();
    let base = server.base();
    let auth = server.state.auth.clone();

    async fn user(auth: &dyn AuthStore, name: &str) -> User {
        auth.create_user(NewUser {
            username: name.into(),
            email: None,
            password_hash: "h".into(),
        })
        .await
        .expect("create user")
    }
    async fn dev(auth: &dyn AuthStore, label: &str, u: &User) -> Device {
        auth.create_device(label, Scope::Interactive, Some(u.id.as_str()))
            .await
            .expect("create device")
    }
    let a = &*auth;

    let owner = user(a, "owner1").await;
    let editor = user(a, "editor1").await;
    let viewer = user(a, "viewer1").await;
    let outsider = user(a, "outsider1").await;

    // Team workspace owned by owner1; editor + viewer added at their roles.
    let team = auth
        .create_workspace(
            NewWorkspace {
                name: "team".into(),
                kind: WorkspaceKind::Team,
                max_pages: None,
                max_bytes: None,
            },
            &owner.id,
        )
        .await
        .expect("team");
    auth.add_member(&team.id, &editor.id, WorkspaceRole::Editor)
        .await
        .expect("add editor");
    auth.add_member(&team.id, &viewer.id, WorkspaceRole::Viewer)
        .await
        .expect("add viewer");

    server
        .state
        .repo
        .create_page(ws_page("p1", &team.id))
        .await
        .expect("page");

    let dev_o: Device = dev(a, "o", &owner).await;
    let dev_e: Device = dev(a, "e", &editor).await;
    let dev_v: Device = dev(a, "v", &viewer).await;
    let dev_x: Device = dev(a, "x", &outsider).await;

    let read_tok = |d: &Device| mint_access(&server.state, &d.id, "read:*", 300);
    let admin_tok = |d: &Device| mint_access(&server.state, &d.id, "admin:*", 300);

    // viewer can read the page.
    let r = http
        .get(format!("{base}/api/pages/p1"))
        .bearer_auth(read_tok(&dev_v))
        .send()
        .await
        .expect("viewer get");
    assert_eq!(r.status().as_u16(), 200, "viewer reads");

    // viewer cannot change it → 403 insufficient_role.
    let r = http
        .post(format!("{base}/api/pages/p1/access"))
        .bearer_auth(admin_tok(&dev_v))
        .json(&json!({ "csp": "off" }))
        .send()
        .await
        .expect("viewer change");
    assert_eq!(r.status().as_u16(), 403, "viewer cannot write");
    let err: Value = r.json().await.expect("json");
    assert_eq!(err["error"].as_str(), Some("insufficient_role"));

    // editor can change it.
    let r = http
        .post(format!("{base}/api/pages/p1/access"))
        .bearer_auth(admin_tok(&dev_e))
        .json(&json!({ "csp": "off" }))
        .send()
        .await
        .expect("editor change");
    assert_eq!(r.status().as_u16(), 200, "editor writes");

    // outsider cannot even read → 403 not_owner.
    let r = http
        .get(format!("{base}/api/pages/p1"))
        .bearer_auth(read_tok(&dev_x))
        .send()
        .await
        .expect("outsider get");
    assert_eq!(r.status().as_u16(), 403, "outsider blocked");
    let err: Value = r.json().await.expect("json");
    assert_eq!(err["error"].as_str(), Some("not_owner"));

    // Transfer + quota: a second workspace with a zero-page quota rejects the
    // incoming page; lifting the quota lets it through and re-homes ownership.
    let t2 = auth
        .create_workspace(
            NewWorkspace {
                name: "t2".into(),
                kind: WorkspaceKind::Team,
                max_pages: Some(0),
                max_bytes: None,
            },
            &owner.id,
        )
        .await
        .expect("t2");

    let r = http
        .post(format!("{base}/api/pages/p1/transfer"))
        .bearer_auth(admin_tok(&dev_o))
        .json(&json!({ "workspace": t2.id }))
        .send()
        .await
        .expect("transfer over quota");
    assert_eq!(r.status().as_u16(), 403, "quota blocks transfer");
    let err: Value = r.json().await.expect("json");
    assert_eq!(err["error"].as_str(), Some("quota_exceeded"));

    auth.set_workspace_quota(&t2.id, None, None)
        .await
        .expect("lift quota");
    let r = http
        .post(format!("{base}/api/pages/p1/transfer"))
        .bearer_auth(admin_tok(&dev_o))
        .json(&json!({ "workspace": t2.id }))
        .send()
        .await
        .expect("transfer ok");
    assert_eq!(r.status().as_u16(), 200, "transfer succeeds");
    let body: Value = r.json().await.expect("json");
    assert_eq!(body["owner_id"].as_str(), Some(t2.id.as_str()));
    assert_eq!(body["owner_kind"].as_str(), Some("workspace"));

    // After the move, the editor (member of `team`, not `t2`) can no longer
    // change the page.
    let r = http
        .post(format!("{base}/api/pages/p1/access"))
        .bearer_auth(admin_tok(&dev_e))
        .json(&json!({ "csp": "strict" }))
        .send()
        .await
        .expect("editor after move");
    assert_eq!(r.status().as_u16(), 403, "editor lost access after transfer");
}
