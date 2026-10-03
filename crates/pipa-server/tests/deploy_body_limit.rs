//! Deploy body-size limits: archives larger than axum's 2 MiB default must be
//! accepted up to `max_upload_bytes`, and anything above it rejected with 413.

mod common;

use pipa_adapters::Config;
use pipa_core::device::Scope;
use reqwest::multipart::{Form, Part};

use crate::common::{mint_access, spawn_test_server, spawn_test_server_with};

/// Zip with `index.html` plus `size` bytes of incompressible data, stored
/// uncompressed so the archive is at least `size` bytes on the wire.
fn make_large_zip(size: usize) -> Vec<u8> {
    use std::io::{Cursor, Write};
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cursor);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zw.start_file("index.html", opts).expect("start index");
        zw.write_all(b"<h1>large</h1>").expect("write index");
        zw.start_file("blob.bin", opts).expect("start blob");
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut blob = Vec::with_capacity(size);
        while blob.len() < size {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            blob.extend_from_slice(&x.to_le_bytes());
        }
        blob.truncate(size);
        zw.write_all(&blob).expect("write blob");
        zw.finish().expect("finish zip");
    }
    cursor.into_inner()
}

/// Deploy `archive` with `access=noauth`. A streamed part has no
/// Content-Length, so the request is sent chunked and the size limit is hit
/// while the handler reads the multipart, not up front by `RequestBodyLimit`.
async fn deploy(server: &common::TestServer, archive: Part) -> reqwest::Response {
    let device = server
        .state
        .auth
        .create_device("ci-runner", Scope::Automation, None)
        .await
        .expect("create device");
    let bearer = mint_access(&server.state, &device.id, "deploy:new", 60);
    let form = Form::new()
        .part(
            "archive",
            archive
                .file_name("site.zip")
                .mime_str("application/zip")
                .expect("mime"),
        )
        .text("access", "noauth");
    reqwest::Client::new()
        .post(format!("{}/api/pages", server.base()))
        .bearer_auth(&bearer)
        .multipart(form)
        .send()
        .await
        .expect("deploy")
}

fn streamed(zip: Vec<u8>) -> Part {
    let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
        zip.chunks(16 * 1024).map(|c| Ok(c.to_vec())).collect();
    Part::stream(reqwest::Body::wrap_stream(futures_util::stream::iter(
        chunks,
    )))
}

#[tokio::test(flavor = "multi_thread")]
async fn archive_above_axum_default_limit_is_accepted() {
    let server = spawn_test_server().await;
    let resp = deploy(&server, Part::bytes(make_large_zip(3 * 1024 * 1024))).await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(
        status, 200,
        "3 MiB deploy under a 100 MiB limit must succeed: {body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn archive_above_configured_limit_is_rejected_with_413() {
    let mut config = Config::default();
    config.hosting.max_upload_bytes = 256 * 1024;
    let server = spawn_test_server_with(config).await;
    let resp = deploy(&server, streamed(make_large_zip(320 * 1024))).await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(
        status, 413,
        "oversized archive must be rejected as too large: {body}"
    );
    assert!(
        body.contains("archive_too_large"),
        "unexpected error body: {body}"
    );
}
