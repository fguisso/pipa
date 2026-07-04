//! Integration tests for `SqliteRepository` against an in-memory SQLite.
//!
//! Goal: verify the persistence behaviour callers depend on — CRUD round-trips
//! plus the more subtle things (FK cascades, stats shape, audit ordering +
//! cap) — without involving any HTTP layer.

mod common;

use pipa_adapters::SqliteRepository;
use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::comment::{CommentStatus, NewComment};
use pipa_core::hit::{HitKind, NewHit};
use pipa_core::page::{Access, Csp, Mode, NewPage, Zone};
use pipa_core::ports::Repository;

use crate::common::{FakeClock, FakeIdGen, setup_in_memory_db};

fn sample_new_page(uuid: &str, ts: i64) -> NewPage {
    NewPage {
        uuid: uuid.to_string(),
        name: Some("hello".into()),
        mode: Mode::Spa,
        access: Access::Noauth,
        zone: Zone::Public,
        password_hash: None,
        owner_kind: "local".into(),
        owner_id: "local".into(),
        size_bytes: 0,
        file_count: 0,
        csp: Csp::Strict,
        created_at: ts,
        updated_at: ts,
    }
}

fn make_repo() -> (FakeRepoEnv,) {
    panic!("unused; tests build repos inline");
}

#[allow(dead_code)]
struct FakeRepoEnv;

#[tokio::test(flavor = "multi_thread")]
async fn page_crud_round_trip_and_cascade() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let repo = SqliteRepository::new(pool.clone(), clock.clone(), id_gen.clone());

    // create
    let created = repo
        .create_page(sample_new_page("page-1", 100))
        .await
        .expect("create_page");
    assert_eq!(created.uuid, "page-1");
    assert_eq!(created.access, Access::Noauth);
    assert_eq!(created.zone, Zone::Public);
    assert_eq!(created.mode, Mode::Spa);

    // find
    let found = repo
        .find_page("page-1")
        .await
        .expect("find_page")
        .expect("page exists");
    assert_eq!(found.uuid, "page-1");

    // list
    let listed = repo.list_pages("local", "local").await.expect("list_pages");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].uuid, "page-1");

    // update access/zone + size
    let mut updated = found.clone();
    updated.access = Access::Password;
    updated.zone = Zone::Private;
    updated.size_bytes = 4096;
    updated.file_count = 12;
    updated.updated_at = 200;
    let saved = repo.update_page(updated).await.expect("update_page");
    assert_eq!(saved.access, Access::Password);
    assert_eq!(saved.zone, Zone::Private);
    assert_eq!(saved.size_bytes, 4096);
    assert_eq!(saved.file_count, 12);
    assert_eq!(saved.updated_at, 200);

    // insert hits + comments first so we can assert cascade-delete
    repo.record_hit(NewHit {
        page_uuid: "page-1".into(),
        ts: 150,
        ip_hash: "iphash".into(),
        ua_hash: None,
        path: "/".into(),
        referrer: None,
        status: 200,
        kind: HitKind::Page,
    })
    .await
    .expect("record_hit");

    let new_comment = NewComment {
        id: "c1".into(),
        page_uuid: "page-1".into(),
        author: "alice".into(),
        body_md: "hi".into(),
        body_html: "<p>hi</p>".into(),
        contact: None,
        ts: 160,
        ip_hash: "iphash".into(),
        status: CommentStatus::Visible,
        user_agent: None,
        anchor_selector: "p:nth-of-type(1)".into(),
        anchor_text: "hello".into(),
        anchor_offset: 0,
    };
    repo.create_comment(new_comment).await.expect("create_comment");

    // sanity: row counts via stats / find_comment
    let stats = repo.stats("page-1", 0).await.expect("stats");
    assert_eq!(stats.views, 1);
    assert!(repo.find_comment("c1").await.expect("find_comment").is_some());

    // delete + cascade
    repo.delete_page("page-1").await.expect("delete_page");
    assert!(repo.find_page("page-1").await.expect("find_page").is_none());

    // FK ON DELETE CASCADE should have removed both child rows.
    let stats_after = repo.stats("page-1", 0).await.expect("stats after");
    assert_eq!(stats_after.views, 0, "hits should cascade-delete");
    assert!(
        repo.find_comment("c1").await.expect("find_comment").is_none(),
        "comments should cascade-delete",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn record_hit_and_stats_shape() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let repo = SqliteRepository::new(pool.clone(), clock.clone(), id_gen.clone());

    repo.create_page(sample_new_page("page-1", 100))
        .await
        .expect("create page");

    // Insert hits across multiple paths/referrers, varied ip_hashes. The
    // `.jpg` asset request is deliberately included: it must NOT inflate views,
    // uniques, top_paths, or top_referrers — only `kind = Page` hits count.
    let cases = [
        ("/", "google.com", "ip1", HitKind::Page),
        ("/", "google.com", "ip2", HitKind::Page),
        ("/", "twitter.com", "ip1", HitKind::Page),
        ("/about", "google.com", "ip3", HitKind::Page),
        ("/about", "google.com", "ip3", HitKind::Page), // duplicate ip+path
        ("/assets/img.jpg", "twitter.com", "ip4", HitKind::Asset),
    ];
    for (i, (path, referrer, ip, kind)) in cases.iter().enumerate() {
        repo.record_hit(NewHit {
            page_uuid: "page-1".into(),
            ts: 200 + i as i64,
            ip_hash: (*ip).into(),
            ua_hash: None,
            path: (*path).into(),
            referrer: Some((*referrer).into()),
            status: 200,
            kind: *kind,
        })
        .await
        .expect("record_hit");
    }

    let stats = repo.stats("page-1", 0).await.expect("stats");
    // 5 page hits ("/" ×3, "/about" ×2); the asset hit is excluded.
    assert_eq!(stats.views, 5, "asset request must not count as a view");
    assert_eq!(stats.uniques, 3, "ip1..ip3 (ip4 only hit an asset)");
    assert_eq!(stats.top_paths.len(), 2, "asset path excluded from top paths");
    // "/" appears 3 times, "/about" 2 → "/" is top
    assert_eq!(stats.top_paths[0].0, "/");
    assert_eq!(stats.top_paths[0].1, 3);
    // Referrers count page hits only: google.com ×4, twitter.com ×1 (its other
    // occurrence was on the excluded asset request).
    assert_eq!(stats.top_referrers[0].0, "google.com");
    assert_eq!(stats.top_referrers[0].1, 4);

    // A self-referrer (the page's own URL, as assets and internal reloads
    // carry) must be scrubbed from the referrer breakdown.
    repo.record_hit(NewHit {
        page_uuid: "page-1".into(),
        ts: 500,
        ip_hash: "ip9".into(),
        ua_hash: None,
        path: "/".into(),
        referrer: Some("http://127.0.0.1:8080/p/page-1/index.html".into()),
        status: 200,
        kind: HitKind::Page,
    })
    .await
    .expect("record_hit self-ref");
    let scrubbed = repo.stats("page-1", 0).await.expect("stats scrubbed");
    assert!(
        scrubbed
            .top_referrers
            .iter()
            .all(|(r, _)| !r.contains("/p/page-1/")),
        "self-referrers must be scrubbed",
    );

    // since_ts filters older rows out (all hits are at ts ≤ 500).
    let recent = repo.stats("page-1", 10_000).await.expect("stats recent");
    assert_eq!(recent.views, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn comment_crud_and_status_transitions_and_rate_count() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let repo = SqliteRepository::new(pool.clone(), clock.clone(), id_gen.clone());
    repo.create_page(sample_new_page("p", 100))
        .await
        .expect("create page");

    // Insert visible + pending + hidden.
    for (id, status, ts) in [
        ("a", CommentStatus::Visible, 110),
        ("b", CommentStatus::Pending, 120),
        ("c", CommentStatus::Hidden, 130),
    ] {
        repo.create_comment(NewComment {
            id: id.into(),
            page_uuid: "p".into(),
            author: "anon".into(),
            body_md: "hi".into(),
            body_html: "<p>hi</p>".into(),
            contact: None,
            ts,
            ip_hash: "iphash".into(),
            status,
            user_agent: None,
            anchor_selector: "p".into(),
            anchor_text: "hi".into(),
            anchor_offset: 0,
        })
        .await
        .expect("create_comment");
    }

    // Default list excludes non-visible.
    let visible = repo.list_comments("p", false).await.expect("list visible");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, "a");

    // include_hidden returns all three.
    let all = repo.list_comments("p", true).await.expect("list all");
    assert_eq!(all.len(), 3);

    // status transition: hide a, then approve b, then re-list visible.
    repo.set_comment_status("a", CommentStatus::Hidden)
        .await
        .expect("hide a");
    repo.set_comment_status("b", CommentStatus::Visible)
        .await
        .expect("approve b");
    let visible_after = repo
        .list_comments("p", false)
        .await
        .expect("list visible after");
    assert_eq!(visible_after.len(), 1);
    assert_eq!(visible_after[0].id, "b");

    // count_recent_comments — by (page, ip_hash) and by ip_hash globally.
    let per_page = repo
        .count_recent_comments("p", "iphash", 0)
        .await
        .expect("count");
    assert_eq!(per_page, 3);
    let per_server = repo
        .count_recent_comments_server("iphash", 0)
        .await
        .expect("count server");
    assert_eq!(per_server, 3);
    // Filter window cuts older rows.
    assert_eq!(
        repo.count_recent_comments("p", "iphash", 200)
            .await
            .expect("count window"),
        0,
    );

    // delete + idempotent absent check.
    repo.delete_comment("a").await.expect("delete comment");
    assert!(repo.find_comment("a").await.expect("find").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn audit_append_and_recent_ordering_and_cap() {
    let pool = setup_in_memory_db().await;
    let clock = FakeClock::arc(1_000);
    let id_gen = FakeIdGen::arc(1);
    let repo = SqliteRepository::new(pool.clone(), clock.clone(), id_gen.clone());

    // Append 250 rows with monotonically-increasing timestamps. recent_audit
    // should cap at 200 and order DESC.
    for i in 0..250i64 {
        repo.record_audit(AuditEvent::success(
            1_000 + i,
            "tester",
            AuditAction::AuthLogin,
        ))
        .await
        .expect("record_audit");
    }
    let rows = repo.recent_audit(0).await.expect("recent_audit");
    assert_eq!(rows.len(), 200, "cap at 200 rows");
    // First row is newest.
    assert_eq!(rows[0].ts, 1_000 + 249);
    // Strictly descending.
    for win in rows.windows(2) {
        assert!(win[0].ts >= win[1].ts, "audit ordering");
    }

    // since_ts filters older rows.
    let only_last_10 = repo.recent_audit(1_000 + 240).await.expect("recent_audit");
    assert_eq!(only_last_10.len(), 10);
}

// Silence dead_code on the unused panic-shim above; it documents that we keep
// the env-free shape so future tests can grab it if needed.
#[allow(dead_code)]
fn _unused() {
    let _ = make_repo;
}
