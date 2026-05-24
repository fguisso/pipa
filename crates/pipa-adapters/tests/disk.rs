//! Integration tests for `DiskStorage`. All FS work happens inside a
//! `tempfile::TempDir` so a failing test never leaves stray files around.

mod common;

use std::sync::Arc;

use bytes::Bytes;
use pipa_adapters::DiskStorage;
use pipa_core::ids::UlidGen;
use pipa_core::ports::Storage;
use tempfile::TempDir;

fn new_storage(tmp: &TempDir) -> DiskStorage {
    let pages_dir = tmp.path().join("pages");
    let trash_dir = tmp.path().join("trash");
    let staging_dir = tmp.path().join("staging");
    // We use a real ULID generator so each staging id is unique within a test.
    DiskStorage::new(pages_dir, trash_dir, staging_dir, Arc::new(UlidGen))
}

#[tokio::test(flavor = "multi_thread")]
async fn stage_promote_and_read_round_trip() {
    let tmp = TempDir::new().expect("tempdir");
    let storage = new_storage(&tmp);

    let h = storage.begin_staging().await.expect("begin");
    storage
        .put_staged(&h, "index.html", Bytes::from_static(b"<h1>hi</h1>"))
        .await
        .expect("put root");
    storage
        .put_staged(
            &h,
            "assets/app.js",
            Bytes::from_static(b"console.log('hi');"),
        )
        .await
        .expect("put nested");
    storage
        .put_staged(
            &h,
            "assets/deep/nested/file.txt",
            Bytes::from_static(b"deep"),
        )
        .await
        .expect("put deeply nested");

    let info = storage.promote(h, "page-a").await.expect("promote");
    assert_eq!(info.file_count, 3);
    assert!(info.size_bytes > 0);

    let body = storage
        .read("page-a", "index.html")
        .await
        .expect("read")
        .expect("present");
    assert_eq!(&body[..], b"<h1>hi</h1>");
    let nested = storage
        .read("page-a", "assets/app.js")
        .await
        .expect("read nested")
        .expect("present");
    assert_eq!(&nested[..], b"console.log('hi');");
    let deep = storage
        .read("page-a", "assets/deep/nested/file.txt")
        .await
        .expect("read deep")
        .expect("present");
    assert_eq!(&deep[..], b"deep");

    // Missing files are Ok(None), not Err.
    assert!(
        storage
            .read("page-a", "nope.html")
            .await
            .expect("read missing")
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn re_promote_moves_old_to_trash() {
    let tmp = TempDir::new().expect("tempdir");
    let storage = new_storage(&tmp);

    // First version.
    let h1 = storage.begin_staging().await.expect("begin1");
    storage
        .put_staged(&h1, "index.html", Bytes::from_static(b"v1"))
        .await
        .expect("put v1");
    storage.promote(h1, "page-x").await.expect("promote v1");

    let trash = tmp.path().join("trash");
    let count_before = count_entries(&trash);

    // Second version replaces.
    let h2 = storage.begin_staging().await.expect("begin2");
    storage
        .put_staged(&h2, "index.html", Bytes::from_static(b"v2"))
        .await
        .expect("put v2");
    storage.promote(h2, "page-x").await.expect("promote v2");

    // Trash should now contain the v1 bundle.
    let count_after = count_entries(&trash);
    assert!(
        count_after > count_before,
        "trash grew from {count_before} to {count_after}"
    );

    let v2 = storage
        .read("page-x", "index.html")
        .await
        .expect("read")
        .expect("present");
    assert_eq!(&v2[..], b"v2", "live page is the new bundle");
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_page_moves_to_trash_and_is_idempotent() {
    let tmp = TempDir::new().expect("tempdir");
    let storage = new_storage(&tmp);

    let h = storage.begin_staging().await.expect("begin");
    storage
        .put_staged(&h, "index.html", Bytes::from_static(b"hi"))
        .await
        .expect("put");
    storage.promote(h, "page-d").await.expect("promote");

    // Sanity: page is readable before delete.
    assert!(
        storage
            .read("page-d", "index.html")
            .await
            .expect("read")
            .is_some()
    );

    storage.delete_page("page-d").await.expect("delete first");

    // After delete, the live read is None.
    assert!(
        storage
            .read("page-d", "index.html")
            .await
            .expect("read post-delete")
            .is_none()
    );

    // Idempotent: deleting an already-deleted page is Ok(()).
    storage.delete_page("page-d").await.expect("delete idempotent");
    storage.delete_page("page-never-existed").await.expect("delete missing");

    // Trash should contain the bundle dir.
    let trash = tmp.path().join("trash");
    assert!(count_entries(&trash) > 0, "trash should hold the bundle");
}

#[tokio::test(flavor = "multi_thread")]
async fn put_staged_rejects_path_traversal() {
    let tmp = TempDir::new().expect("tempdir");
    let storage = new_storage(&tmp);
    let h = storage.begin_staging().await.expect("begin");

    // Path traversal via `..`.
    let err = storage
        .put_staged(&h, "../etc/passwd", Bytes::from_static(b"pwn"))
        .await
        .expect_err("traversal must error");
    assert!(matches!(err, pipa_core::CoreError::InvalidInput(_)));

    // Absolute path.
    let err = storage
        .put_staged(&h, "/etc/passwd", Bytes::from_static(b"pwn"))
        .await
        .expect_err("absolute must error");
    assert!(matches!(err, pipa_core::CoreError::InvalidInput(_)));

    // The escape attempt must NOT have created anything outside staging.
    let leaked = tmp.path().parent().expect("tmp parent").join("etc").join("passwd");
    assert!(
        !leaked.exists(),
        "path traversal wrote outside staging: {}",
        leaked.display()
    );
    // Also: no `etc/passwd` inside our tempdir.
    let inside = tmp.path().join("etc").join("passwd");
    assert!(!inside.exists(), "traversal materialised inside tmpdir");
}

fn count_entries(dir: &std::path::Path) -> usize {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.count(),
        Err(_) => 0,
    }
}
