//! Best-effort page-thumbnail capture for the admin dashboard.
//!
//! Compiled only under the `thumbnails` feature — it is heavy: it shells out to
//! a headless Chromium. After a deploy we screenshot the page and cache the PNG
//! so the dashboard can show a preview.
//!
//! We deliberately do NOT reuse the public serving path (that would drag in the
//! auth gate and the CSP layer). Instead we spin up a throwaway static-file
//! server bound to `127.0.0.1:0`, rooted at the page's on-disk bundle, point
//! Chromium at it, and tear the server down. It lives for exactly one capture
//! and is reachable only over loopback — we already own auth, so an internal,
//! ephemeral, unauthenticated view of our own files is fine, and gated/private
//! pages get a real screenshot instead of a lock screen.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::Router;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tower_http::services::ServeDir;

use crate::state::AppState;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);

/// Capture a thumbnail for `uuid`. Best-effort: any failure (Chromium missing,
/// a missing bundle, a non-zero exit, a timeout) is logged and swallowed —
/// never propagated — so a deploy is never affected by thumbnailing.
pub async fn capture(state: AppState, uuid: String) {
    if let Err(e) = try_capture(&state, &uuid).await {
        tracing::warn!(uuid = %uuid, error = %e, "thumbnail capture failed");
    }
}

async fn try_capture(state: &AppState, uuid: &str) -> anyhow::Result<()> {
    let cfg = &state.config.thumbnails;
    let bundle_dir: PathBuf = state.config.server.pages_dir.join(uuid);
    if !bundle_dir.is_dir() {
        anyhow::bail!("no bundle dir at {}", bundle_dir.display());
    }

    // Persist under <data_dir>/thumbnails/<uuid>.png — outside the page bundle
    // so it is never publicly servable at /p/<uuid>/... Chromium writes a temp
    // file we atomically rename into place, so the dashboard never sees a
    // half-written PNG.
    let dir = state.config.server.data_dir.join("thumbnails");
    tokio::fs::create_dir_all(&dir).await?;
    let tmp_path = dir.join(format!(".{uuid}.tmp.png"));
    let final_path = dir.join(format!("{uuid}.png"));

    // Ephemeral loopback static server over the page's files; torn down whatever
    // the capture outcome.
    let (port, server) = spawn_static_server(bundle_dir).await?;
    let shot = run_chromium(&cfg.chromium_path, cfg.width, cfg.height, port, &tmp_path).await;
    server.abort();

    match shot {
        Ok(()) => {
            tokio::fs::rename(&tmp_path, &final_path).await?;
            tracing::info!(uuid = %uuid, "thumbnail captured");
            Ok(())
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            Err(e)
        }
    }
}

/// Bind a static-file server to `127.0.0.1:0`, serving `dir`, and return the
/// bound port plus the task handle (abort it to shut the server down).
async fn spawn_static_server(dir: PathBuf) -> anyhow::Result<(u16, JoinHandle<()>)> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
    let port = listener.local_addr()?.port();
    let app = Router::new().fallback_service(ServeDir::new(dir));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok((port, handle))
}

/// Removes a directory tree on drop — used for the throwaway Chromium profile so
/// it's cleaned up whether the capture succeeds, errors, or times out.
struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Drive headless Chromium to screenshot `http://127.0.0.1:<port>/` into
/// `out_path`. Errors on a missing binary, non-zero exit, empty/absent output,
/// or timeout.
async fn run_chromium(
    chromium_path: &str,
    width: u32,
    height: u32,
    port: u16,
    out_path: &Path,
) -> anyhow::Result<()> {
    // Give Chromium a disposable, isolated profile. Without an explicit
    // `--user-data-dir`, Chrome's first-run integration hangs indefinitely under
    // a fresh/empty HOME (common in servers/containers), blowing the timeout; and
    // two concurrent captures would fight over the default profile's lock. Keyed
    // by the ephemeral port, which is unique per in-flight capture.
    let profile_dir = std::env::temp_dir().join(format!("pipa-thumb-{port}"));
    let _profile = DirGuard(profile_dir.clone());

    let mut cmd = tokio::process::Command::new(chromium_path);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--hide-scrollbars")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--window-size={width},{height}"))
        .arg("--virtual-time-budget=4000")
        .arg(format!("--screenshot={}", out_path.display()))
        .arg(format!("http://127.0.0.1:{port}/"))
        // Kill the Chromium process tree if the timeout below drops this future,
        // instead of leaking an orphan that keeps running against a dead port.
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());

    // `spawn` fails fast if the Chromium binary is absent — the caller logs it.
    let child = cmd.spawn()?;
    let output = tokio::time::timeout(CAPTURE_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| {
            anyhow::anyhow!("chromium timed out after {}s", CAPTURE_TIMEOUT.as_secs())
        })??;
    if !output.status.success() {
        anyhow::bail!(
            "chromium exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let len = tokio::fs::metadata(out_path)
        .await
        .map_err(|_| anyhow::anyhow!("chromium wrote no screenshot"))?
        .len();
    if len == 0 {
        anyhow::bail!("chromium produced an empty screenshot");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    // The ephemeral static server is the security-critical, chromium-free core:
    // it must bind on loopback, serve the bundle's index.html, and shut down.
    #[tokio::test(flavor = "multi_thread")]
    async fn ephemeral_server_serves_index_then_shuts_down() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<h1>hello-thumb</h1>").unwrap();

        let (port, handle) = spawn_static_server(dir.path().to_path_buf())
            .await
            .expect("spawn ephemeral server");

        // Raw HTTP/1.0 GET so we don't depend on an HTTP client dev-dep.
        let body = tokio::task::spawn_blocking(move || {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
            let mut buf = String::new();
            s.read_to_string(&mut buf).unwrap();
            buf
        })
        .await
        .unwrap();

        assert!(body.contains("200"), "expected 200, got: {body}");
        assert!(body.contains("hello-thumb"), "index body should be served");

        // Shut it down; the port should stop accepting connections.
        handle.abort();
        let _ = handle.await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let closed = tokio::task::spawn_blocking(move || {
            TcpStream::connect(("127.0.0.1", port)).is_err()
        })
        .await
        .unwrap();
        assert!(closed, "ephemeral server port should be closed after abort");
    }
}
