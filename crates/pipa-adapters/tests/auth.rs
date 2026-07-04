//! Integration tests for `SqliteAuthStore`. Same in-memory SQLite + fake
//! clock pattern as the repository tests; the focus here is the more
//! procedural flows (setup-code → pairing → refresh rotation → step-up) and
//! the security-critical "double-consume rejected" assertions.

mod common;

use pipa_adapters::SqliteAuthStore;
use pipa_core::device::Scope;
use pipa_core::ports::{AuthStore, PollResult};
use pipa_core::user::NewUser;
use pipa_core::workspace::{NewWorkspace, WorkspaceKind, WorkspaceRole};

use crate::common::{FakeClock, FakeIdGen, setup_in_memory_db};

#[tokio::test(flavor = "multi_thread")]
async fn setup_code_issue_consume_and_rejections() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let auth = SqliteAuthStore::new(pool.clone(), clock.clone(), id_gen.clone());

    // Happy path.
    let setup = auth.issue_setup_code().await.expect("issue setup");
    assert!(setup.expires_at > setup.created_at);
    let consumed = auth.consume_setup_code(&setup.code).await.expect("consume");
    assert!(consumed, "first consume must succeed");

    // Double-consume must fail.
    let double = auth.consume_setup_code(&setup.code).await.expect("consume");
    assert!(!double, "double-consume must fail");

    // Expired: issue a fresh one, advance clock past expiry, consume should fail.
    let setup2 = auth.issue_setup_code().await.expect("issue setup 2");
    clock.add(60 * 60 * 24); // +1 day, well past the 15-min TTL
    let after_expiry = auth.consume_setup_code(&setup2.code).await.expect("consume");
    assert!(!after_expiry, "expired setup code must not be consumable");

    // Unknown code: false, not error.
    assert!(
        !auth
            .consume_setup_code("0000-0000")
            .await
            .expect("consume unknown")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn device_pairing_full_cycle_with_expiry() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let auth = SqliteAuthStore::new(pool.clone(), clock.clone(), id_gen.clone());

    let (code, secret) = auth.begin_pairing().await.expect("begin pairing");

    // Poll before approve → Pending.
    match auth.poll_pairing(&code, &secret).await.expect("poll pending") {
        PollResult::Pending => {}
        other => panic!("expected Pending, got {other:?}"),
    }

    // Approve → returns issued refresh.
    let issued = auth
        .approve_pairing(&code, "Test Mac", Scope::Interactive, None)
        .await
        .expect("approve");
    assert!(!issued.refresh_plaintext.is_empty());
    assert_eq!(issued.device.label, "Test Mac");
    assert_eq!(issued.device.scope, Scope::Interactive);

    // Poll after approve → Approved + same plaintext.
    match auth.poll_pairing(&code, &secret).await.expect("poll approved") {
        PollResult::Approved(again) => {
            assert_eq!(again.refresh_plaintext, issued.refresh_plaintext);
            assert_eq!(again.device.id, issued.device.id);
        }
        other => panic!("expected Approved, got {other:?}"),
    }

    // Wrong secret → Unauthorized.
    let err = auth
        .poll_pairing(&code, "deadbeef")
        .await
        .expect_err("wrong secret must Unauthorized");
    assert!(matches!(err, pipa_core::CoreError::Unauthorized));

    // Expired (fresh pairing): advance clock, then poll.
    let (code2, secret2) = auth.begin_pairing().await.expect("begin pairing 2");
    clock.add(60 * 60 * 24); // +1 day, past the 10-min TTL
    match auth.poll_pairing(&code2, &secret2).await.expect("poll expired") {
        PollResult::Expired => {}
        other => panic!("expected Expired, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_rotation_invalidates_original() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let auth = SqliteAuthStore::new(pool.clone(), clock.clone(), id_gen.clone());

    let device = auth
        .create_device("CI", Scope::Automation, None)
        .await
        .expect("create device");
    let (orig_token, orig_plain) = auth
        .issue_refresh(&device.id, Scope::Automation, 3600)
        .await
        .expect("issue refresh");

    // Lookup of the original plaintext succeeds.
    assert!(
        auth.lookup_refresh(&orig_plain)
            .await
            .expect("lookup before")
            .is_some()
    );

    // Rotate. New plaintext is different, original is invalidated.
    let (new_token, new_plain) = auth.rotate_refresh(&orig_plain).await.expect("rotate");
    assert_ne!(new_plain, orig_plain);
    assert_ne!(new_token.id, orig_token.id);

    // Original now fails to lookup.
    assert!(
        auth.lookup_refresh(&orig_plain)
            .await
            .expect("lookup after")
            .is_none(),
        "old refresh must be invalidated after rotation"
    );

    // New plaintext resolves.
    assert!(
        auth.lookup_refresh(&new_plain)
            .await
            .expect("lookup new")
            .is_some()
    );

    // Rotating the already-rotated original is rejected.
    let err = auth
        .rotate_refresh(&orig_plain)
        .await
        .expect_err("rotate already-rotated");
    assert!(matches!(err, pipa_core::CoreError::Unauthorized));
}

#[tokio::test(flavor = "multi_thread")]
async fn revoke_device_cascades_to_refresh_tokens() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let auth = SqliteAuthStore::new(pool.clone(), clock.clone(), id_gen.clone());

    let device = auth
        .create_device("laptop", Scope::Interactive, None)
        .await
        .expect("create");
    let (_t, plain) = auth
        .issue_refresh(&device.id, Scope::Interactive, 3600)
        .await
        .expect("issue refresh");
    assert!(
        auth.lookup_refresh(&plain).await.expect("lookup").is_some()
    );

    auth.revoke_device(&device.id).await.expect("revoke");

    // Refresh should be cascade-revoked.
    assert!(
        auth.lookup_refresh(&plain)
            .await
            .expect("lookup after")
            .is_none(),
        "revoking a device should invalidate its refresh tokens"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn step_up_init_confirm_consume_and_rejections() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let auth = SqliteAuthStore::new(pool.clone(), clock.clone(), id_gen.clone());

    let device = auth
        .create_device("laptop", Scope::Interactive, None)
        .await
        .expect("create device");

    let token = auth
        .begin_step_up(&device.id, "page.delete", Some("page-1"), Some("iphash"))
        .await
        .expect("begin");

    // Cannot consume before confirm.
    let too_early = auth
        .consume_step_up(&token.code, &device.id, "page.delete", Some("page-1"))
        .await
        .expect("consume early");
    assert!(!too_early, "must not consume before confirm");

    // Confirm via the browser-side flow.
    auth.confirm_step_up(&token.code).await.expect("confirm");

    // Mismatching args fail.
    let wrong_op = auth
        .consume_step_up(&token.code, &device.id, "page.delete", Some("page-OTHER"))
        .await
        .expect("consume wrong target");
    assert!(!wrong_op);
    let wrong_device = auth
        .consume_step_up(&token.code, "other-device", "page.delete", Some("page-1"))
        .await
        .expect("consume wrong device");
    assert!(!wrong_device);

    // Matching consume succeeds…
    let ok = auth
        .consume_step_up(&token.code, &device.id, "page.delete", Some("page-1"))
        .await
        .expect("consume happy");
    assert!(ok);

    // …and double-consume is rejected.
    let again = auth
        .consume_step_up(&token.code, &device.id, "page.delete", Some("page-1"))
        .await
        .expect("consume again");
    assert!(!again, "double-consume must fail");

    // Confirming a long-expired token is rejected.
    let t2 = auth
        .begin_step_up(&device.id, "page.delete", Some("page-2"), None)
        .await
        .expect("begin 2");
    clock.add(60 * 60 * 24);
    let err = auth.confirm_step_up(&t2.code).await.expect_err("confirm expired");
    assert!(matches!(err, pipa_core::CoreError::Unauthorized));
}

#[tokio::test(flavor = "multi_thread")]
async fn users_sessions_device_linkage_and_oauth() {
    use pipa_core::user::{NewOAuthIdentity, NewUser, OAuthProvider};

    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let auth = SqliteAuthStore::new(pool.clone(), clock.clone(), id_gen.clone());

    // create + find
    let user = auth
        .create_user(NewUser {
            username: "alice".into(),
            email: Some("a@example.com".into()),
            password_hash: "argon$hash".into(),
        })
        .await
        .expect("create_user");
    assert_eq!(user.username, "alice");
    assert!(user.disabled_at.is_none());

    let by_name = auth
        .find_user_by_username("alice")
        .await
        .expect("find")
        .expect("some");
    assert_eq!(by_name.id, user.id);
    let by_id = auth.find_user_by_id(&user.id).await.expect("find").expect("some");
    assert_eq!(by_id.username, "alice");

    // duplicate username → AlreadyExists
    let dup = auth
        .create_user(NewUser {
            username: "alice".into(),
            email: None,
            password_hash: "x".into(),
        })
        .await;
    assert!(matches!(dup, Err(pipa_core::CoreError::AlreadyExists)));

    // list
    assert_eq!(auth.list_users().await.expect("list").len(), 1);

    // disable/enable
    auth.set_user_disabled(&user.id, true).await.expect("disable");
    assert!(auth.find_user_by_id(&user.id).await.unwrap().unwrap().disabled_at.is_some());
    auth.set_user_disabled(&user.id, false).await.expect("enable");
    assert!(auth.find_user_by_id(&user.id).await.unwrap().unwrap().disabled_at.is_none());

    // user session lifecycle
    let s = auth
        .create_user_session(&user.id, Some("agent"), Some("127.0.0.1"))
        .await
        .expect("create_user_session");
    assert_eq!(auth.find_user_session(&s.id).await.unwrap().unwrap().user_id, user.id);
    assert_eq!(auth.list_user_sessions(&user.id).await.unwrap().len(), 1);
    auth.revoke_user_session(&s.id).await.expect("revoke");
    assert!(auth.find_user_session(&s.id).await.unwrap().is_none(), "revoked session hidden");

    // device → user linkage: a device paired to the user resolves back to it.
    let dev = auth
        .create_device("alice-laptop", Scope::Interactive, Some(user.id.as_str()))
        .await
        .expect("create_device with user");
    assert_eq!(dev.user_id.as_deref(), Some(user.id.as_str()));
    let user_devs = auth.list_devices_for_user(&user.id).await.expect("list dev");
    assert_eq!(user_devs.len(), 1);
    assert_eq!(user_devs[0].id, dev.id);

    // set_device_user on a previously-unlinked device
    let orphan = auth
        .create_device("ci", Scope::Automation, None)
        .await
        .expect("create orphan");
    assert!(orphan.user_id.is_none());
    auth.set_device_user(&orphan.id, &user.id).await.expect("link");
    assert_eq!(auth.list_devices_for_user(&user.id).await.unwrap().len(), 2);

    // oauth scaffold: link + find
    auth.link_oauth(NewOAuthIdentity {
        user_id: user.id.clone(),
        provider: OAuthProvider::Github,
        subject: "gh-42".into(),
    })
    .await
    .expect("link_oauth");
    let found = auth
        .find_user_by_oauth(OAuthProvider::Github, "gh-42")
        .await
        .expect("find_by_oauth")
        .expect("some");
    assert_eq!(found.id, user.id);
    assert!(
        auth.find_user_by_oauth(OAuthProvider::Google, "gh-42")
            .await
            .expect("find")
            .is_none(),
        "provider is part of the key"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn workspaces_membership_roles_and_quota() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let auth = SqliteAuthStore::new(pool.clone(), clock.clone(), id_gen.clone());

    let new_user = |name: &str| NewUser {
        username: name.to_string(),
        email: None,
        password_hash: "h".into(),
    };

    // create_user seeds a personal workspace with the user as owner.
    let alice = auth.create_user(new_user("alice")).await.expect("alice");
    let bob = auth.create_user(new_user("bob")).await.expect("bob");

    let a_ws = auth
        .list_workspaces_for_user(&alice.id)
        .await
        .expect("alice ws");
    assert_eq!(a_ws.len(), 1, "one personal workspace");
    assert_eq!(a_ws[0].role, WorkspaceRole::Owner);
    assert_eq!(a_ws[0].workspace.id, format!("ws-{}", alice.id));

    // A team workspace, owned by alice.
    let team = auth
        .create_workspace(
            NewWorkspace {
                name: "team".into(),
                kind: WorkspaceKind::Team,
                max_pages: None,
                max_bytes: None,
            },
            &alice.id,
        )
        .await
        .expect("team");
    assert_eq!(
        auth.get_member_role(&team.id, &alice.id).await.unwrap(),
        Some(WorkspaceRole::Owner)
    );
    // Alice now belongs to personal + team.
    assert_eq!(
        auth.list_workspaces_for_user(&alice.id).await.unwrap().len(),
        2
    );

    // Add bob as editor, then demote to viewer.
    auth.add_member(&team.id, &bob.id, WorkspaceRole::Editor)
        .await
        .expect("add bob");
    assert_eq!(
        auth.get_member_role(&team.id, &bob.id).await.unwrap(),
        Some(WorkspaceRole::Editor)
    );
    auth.update_member_role(&team.id, &bob.id, WorkspaceRole::Viewer)
        .await
        .expect("demote bob");
    assert_eq!(
        auth.get_member_role(&team.id, &bob.id).await.unwrap(),
        Some(WorkspaceRole::Viewer)
    );

    // Members list joins usernames.
    let members = auth.list_members(&team.id).await.expect("members");
    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|m| m.username == "alice" && m.role == WorkspaceRole::Owner));
    assert!(members.iter().any(|m| m.username == "bob" && m.role == WorkspaceRole::Viewer));

    // add_member on an unknown user is a clean NotFound.
    assert!(auth
        .add_member(&team.id, "nope", WorkspaceRole::Viewer)
        .await
        .is_err());

    // Quota round-trips.
    auth.set_workspace_quota(&team.id, Some(5), Some(1024))
        .await
        .expect("quota");
    let got = auth.get_workspace(&team.id).await.unwrap().unwrap();
    assert_eq!(got.max_pages, Some(5));
    assert_eq!(got.max_bytes, Some(1024));

    // Remove bob.
    auth.remove_member(&team.id, &bob.id).await.expect("remove bob");
    assert_eq!(auth.get_member_role(&team.id, &bob.id).await.unwrap(), None);
}
