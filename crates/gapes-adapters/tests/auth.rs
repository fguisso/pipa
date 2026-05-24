//! Integration tests for `SqliteAuthStore`. Same in-memory SQLite + fake
//! clock pattern as the repository tests; the focus here is the more
//! procedural flows (setup-code → pairing → refresh rotation → step-up) and
//! the security-critical "double-consume rejected" assertions.

mod common;

use gapes_adapters::SqliteAuthStore;
use gapes_core::device::Scope;
use gapes_core::ports::{AuthStore, PollResult};

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
        .approve_pairing(&code, "Test Mac", Scope::Interactive)
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
    assert!(matches!(err, gapes_core::CoreError::Unauthorized));

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
        .create_device("CI", Scope::Automation)
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
    assert!(matches!(err, gapes_core::CoreError::Unauthorized));
}

#[tokio::test(flavor = "multi_thread")]
async fn revoke_device_cascades_to_refresh_tokens() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let auth = SqliteAuthStore::new(pool.clone(), clock.clone(), id_gen.clone());

    let device = auth
        .create_device("laptop", Scope::Interactive)
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
        .create_device("laptop", Scope::Interactive)
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
    assert!(matches!(err, gapes_core::CoreError::Unauthorized));
}
