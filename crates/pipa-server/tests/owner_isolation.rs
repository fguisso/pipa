//! Phase 3 owner isolation: a `user` may only read/list/delete its own pages;
//! the single-owner ("local") identity is a superuser. Authz is resolved from
//! the caller's device → user linkage, independent of scope.

mod common;

use pipa_core::device::Scope;
use pipa_core::page::{Access, Csp, Mode, NewPage, Zone};
use pipa_core::user::NewUser;
use serde_json::Value;

use crate::common::{mint_access, spawn_test_server};

fn sample_page(uuid: &str, owner_kind: &str, owner_id: &str) -> NewPage {
    NewPage {
        uuid: uuid.to_string(),
        name: Some(uuid.to_string()),
        mode: Mode::Spa,
        access: Access::Noauth,
        zone: Zone::Public,
        password_hash: None,
        owner_kind: owner_kind.to_string(),
        owner_id: owner_id.to_string(),
        size_bytes: 0,
        file_count: 0,
        csp: Csp::Strict,
        created_at: 1000,
        updated_at: 1000,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn users_are_isolated_and_local_is_superuser() {
    let server = spawn_test_server().await;
    let http = reqwest::Client::new();
    let base = server.base();

    // Two users, each with a linked CLI device.
    let alice = server
        .state
        .auth
        .create_user(NewUser {
            username: "alice".into(),
            email: None,
            password_hash: "h".into(),
        })
        .await
        .expect("create alice");
    let bob = server
        .state
        .auth
        .create_user(NewUser {
            username: "bob".into(),
            email: None,
            password_hash: "h".into(),
        })
        .await
        .expect("create bob");

    let dev_a = server
        .state
        .auth
        .create_device("alice-cli", Scope::Interactive, Some(alice.id.as_str()))
        .await
        .expect("dev a");
    let dev_b = server
        .state
        .auth
        .create_device("bob-cli", Scope::Interactive, Some(bob.id.as_str()))
        .await
        .expect("dev b");
    // An unlinked device = the single-owner "local" superuser.
    let dev_local = server
        .state
        .auth
        .create_device("admin-ui", Scope::Interactive, None)
        .await
        .expect("dev local");

    // One page per user, owned by each user's personal workspace (Phase 4:
    // `create_user` seeds `ws-<userid>` and makes the user its owner).
    server
        .state
        .repo
        .create_page(sample_page(
            "page-alice",
            "workspace",
            &format!("ws-{}", alice.id),
        ))
        .await
        .expect("page alice");
    server
        .state
        .repo
        .create_page(sample_page("page-bob", "workspace", &format!("ws-{}", bob.id)))
        .await
        .expect("page bob");

    let a_read = mint_access(&server.state, &dev_a.id, "read:*", 300);
    let b_read = mint_access(&server.state, &dev_b.id, "read:*", 300);
    let b_destroy = mint_access(&server.state, &dev_b.id, "destroy:*", 300);
    let local_read = mint_access(&server.state, &dev_local.id, "read:*", 300);

    let get = |token: &str, uuid: &str| {
        http.get(format!("{base}/api/pages/{uuid}"))
            .bearer_auth(token)
            .send()
    };

    // Bob cannot read Alice's page → 403 not_owner.
    let r = get(&b_read, "page-alice").await.expect("get");
    assert_eq!(r.status().as_u16(), 403, "bob must not read alice's page");
    let err: Value = r.json().await.expect("err json");
    assert_eq!(err["error"].as_str(), Some("not_owner"));

    // Alice can read her own page.
    assert_eq!(
        get(&a_read, "page-alice").await.expect("get").status().as_u16(),
        200,
        "alice reads her own page"
    );

    // Local (unlinked device) is a superuser → can read Alice's page.
    assert_eq!(
        get(&local_read, "page-alice").await.expect("get").status().as_u16(),
        200,
        "local identity is a superuser"
    );

    // Listing is scoped: Alice sees only her page.
    let list: Value = http
        .get(format!("{base}/api/pages"))
        .bearer_auth(&a_read)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("list json");
    let pages = list["pages"].as_array().expect("pages array");
    assert_eq!(pages.len(), 1, "alice lists only her own page");
    assert_eq!(pages[0]["uuid"].as_str(), Some("page-alice"));

    // Bob deleting Alice's page is rejected on ownership (before any step-up).
    let del = http
        .delete(format!("{base}/api/pages/page-alice"))
        .bearer_auth(&b_destroy)
        .send()
        .await
        .expect("delete");
    assert_eq!(del.status().as_u16(), 403, "bob cannot delete alice's page");
    let err: Value = del.json().await.expect("err json");
    assert_eq!(err["error"].as_str(), Some("not_owner"));
}
