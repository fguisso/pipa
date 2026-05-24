//! Shared test harness for the `pipa-server` integration tests.
//!
//! `spawn_test_server` builds an `AppState` against in-memory SQLite + a real
//! `DiskStorage` rooted at a fresh tempdir, binds the router to a random
//! loopback port, and returns the bound address plus the live state so the
//! test can mint tokens and seed rows directly.

use std::net::SocketAddr;
use std::path::PathBuf;

use pipa_adapters::Config;
use pipa_server::{AppState, build_app_state_for_test, build_router};
use tempfile::TempDir;
use tokio::net::TcpListener;

// Each integration test binary compiles `common/mod.rs` independently, so any
// helper not used by a particular binary trips dead-code warnings. We mark
// them explicitly — these helpers are deliberately part of the shared surface.
#[allow(dead_code)]
pub struct TestServer {
    pub addr: SocketAddr,
    pub state: AppState,
    // Hold the tempdir alive for the lifetime of the test.
    pub data_root: TempDir,
}

impl TestServer {
    #[allow(dead_code)]
    pub fn base(&self) -> String {
        format!("http://{}", self.addr)
    }
}

/// Spin up a real server on a random loopback port. Returns a handle the test
/// can issue requests against via `reqwest`. The state is also handed back so
/// the test can mint tokens or insert rows directly (bypassing the HTTP
/// surface where that's the cleaner setup path).
#[allow(dead_code)]
pub async fn spawn_test_server() -> TestServer {
    spawn_test_server_with(Config::default()).await
}

#[allow(dead_code)]
pub async fn spawn_test_server_with(mut config: Config) -> TestServer {
    let data_root = TempDir::new().expect("data tempdir");
    let pages_dir: PathBuf = data_root.path().join("pages");

    // Dev-mode keeps cookies un-Secure so we can drive password flows over
    // plain http inside the test.
    config.server.dev = true;
    // Public URL is used by the password-cookie redirect and the deploy
    // response. Setting it explicitly keeps assertions stable.
    if config.server.public_url.is_empty() {
        config.server.public_url = "http://127.0.0.1".into();
    }

    let state = build_app_state_for_test(pages_dir, config)
        .await
        .expect("build state");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local addr");

    let router = build_router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });

    TestServer {
        addr,
        state,
        data_root,
    }
}

/// Mint an access token for `(sub, scope)` against the test HMAC key.
#[allow(dead_code)]
pub fn mint_access(state: &AppState, sub: &str, scope: &str, ttl_sec: i64) -> String {
    let (tok, _claims) =
        pipa_server::auth::tokens::mint_access_token(&state.hmac_key, sub, scope, ttl_sec)
            .expect("mint access");
    tok
}

/// Build a single-file zip with `index.html` containing `body`. Returned as
/// `Vec<u8>` ready to attach to a multipart form. Avoids a separate zip-helper
/// crate; we lean on the same `zip` workspace dep the server uses.
#[allow(dead_code)]
pub fn make_zip_with_index(body: &str) -> Vec<u8> {
    use std::io::{Cursor, Write};
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cursor);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zw.start_file("index.html", opts).expect("start_file");
        zw.write_all(body.as_bytes()).expect("write index");
        zw.finish().expect("finish zip");
    }
    cursor.into_inner()
}
