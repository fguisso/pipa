use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::Router;
use axum::body::Body;
use axum::extract::{Form, OriginalUri, Path, State};
use axum::http::header::{HeaderMap, HeaderValue};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::Engine as _;
use bytes::Bytes;
use pipa_adapters::verify_password;
use pipa_core::{Access, Csp, Mode, NewHit, Page};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::auth::OwnerCookieOpt;
use crate::auth::tokens::mint_access_token;
use crate::error::ServerError;
use crate::ip_hash::{hmac_ip, hmac_value};
use crate::middleware::forwarded::{ProxyPeer, RealIp};
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

/// Password cookie TTL — short and refreshed on every visit. Matches the
/// "5-minute" budget for step-up cookies; same threat model (compromised
/// browser shouldn't keep access forever).
const PASSWORD_COOKIE_TTL_SECS: i64 = 300;
const COOKIE_NAME_PREFIX: &str = "gpages_p_";

#[derive(Template)]
#[template(path = "password_gate.html")]
struct PasswordGateTemplate<'a> {
    post_action: &'a str,
    next_path: &'a str,
    error: bool,
}

pub fn router(_state: AppState) -> Router<AppState> {
    // Note: the previous incarnation of this router wrapped every response in
    // `PageCspLayer` for defense-in-depth. We now emit the CSP header per
    // response (only when `page.csp == Csp::Strict`) so the per-page `off`
    // opt-out actually takes effect. `render_gate` always emits CSP — the
    // gate is our HTML, not the page owner's, so it stays locked down.
    Router::new()
        .route("/__gate.css", get(gate_css))
        .route("/p/:uuid/__gate", post(submit_gate))
        .route("/p/:uuid", get(redirect_to_trailing_slash))
        .route("/p/:uuid/", get(serve_root))
        .route("/p/:uuid/*path", get(serve_path))
}

/// Stylesheet for the password gate, served from our own origin so the strict
/// gate CSP (`default-src 'self'`) permits it. Inlining the CSS into the gate
/// HTML would require `style-src 'unsafe-inline'` (or a nonce), which we don't
/// want to weaken the policy for. Static and shared across all gated pages, so
/// it gets a long-lived cache.
async fn gate_css() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(include_str!("../../templates/gate.css")))
        .expect("static gate css response builds")
}

/// Bug A fix: relative URLs in `index.html` (e.g. `href="css/site.css"`) are
/// resolved against the *parent* of the request URL. Without a trailing slash
/// the browser asks for `/p/css/site.css` instead of `/p/<uuid>/css/site.css`,
/// gets a 404, and refuses the asset under strict MIME. 308 keeps the method
/// + body intact and is the correct redirect kind for a "this URL has moved"
/// semantic.
///
/// We deliberately verify the page exists before redirecting: an unconditional
/// 308 on `/p/<garbage>` would leak nothing meaningful but would still be a
/// surprising answer to "is this a real URL" — 404 here matches what the
/// canonical `/p/<uuid>/` endpoint would return for the same UUID.
async fn redirect_to_trailing_slash(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    match state.repo.find_page(&uuid).await {
        Ok(Some(_)) => {}
        _ => return not_found_response(),
    }
    let location = match uri.query() {
        Some(q) if !q.is_empty() => format!("/p/{uuid}/?{q}"),
        _ => format!("/p/{uuid}/"),
    };
    let mut resp = Response::builder()
        .status(StatusCode::PERMANENT_REDIRECT)
        .body(Body::empty())
        .expect("static redirect");
    if let Ok(v) = HeaderValue::from_str(&location) {
        resp.headers_mut().insert(header::LOCATION, v);
    }
    resp
}

#[derive(Debug, Deserialize)]
pub struct GateForm {
    pub password: String,
    #[serde(default)]
    pub next: String,
}

async fn serve_root(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    real_ip: RealIp,
    proxy_peer: ProxyPeer,
    headers: HeaderMap,
    jar: CookieJar,
    owner: OwnerCookieOpt,
) -> Response {
    serve(state, uuid, String::new(), real_ip, proxy_peer.0, headers, jar, owner).await
}

async fn serve_path(
    State(state): State<AppState>,
    Path((uuid, path)): Path<(String, String)>,
    real_ip: RealIp,
    proxy_peer: ProxyPeer,
    headers: HeaderMap,
    jar: CookieJar,
    owner: OwnerCookieOpt,
) -> Response {
    serve(state, uuid, path, real_ip, proxy_peer.0, headers, jar, owner).await
}

#[allow(clippy::too_many_arguments)]
async fn serve(
    state: AppState,
    uuid: String,
    rel_path: String,
    real_ip: RealIp,
    peer_ip: Option<String>,
    headers: HeaderMap,
    jar: CookieJar,
    owner: OwnerCookieOpt,
) -> Response {
    match serve_inner(state, uuid, rel_path, real_ip, peer_ip, headers, jar, owner).await {
        Ok(resp) => resp,
        Err(e) => e.into_response(),
    }
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(feature = "zone"), allow(unused_variables))]
async fn serve_inner(
    state: AppState,
    uuid: String,
    rel_path: String,
    real_ip: RealIp,
    peer_ip: Option<String>,
    headers: HeaderMap,
    jar: CookieJar,
    owner: OwnerCookieOpt,
) -> Result<Response, ServerError> {
    let page = match state.repo.find_page(&uuid).await? {
        Some(p) => p,
        None => return Ok(not_found_response()),
    };

    // Archived pages are soft-unpublished — 404 regardless of anything else so
    // the bundle stops being addressable but stays on disk for un-archive.
    if page.archived {
        return Ok(not_found_response());
    }

    // Zone enforcement (the "where" axis), an exact match: a page serves only on
    // the single channel its zone maps to; a mismatch is a silent 404 so the
    // page's existence never leaks across the boundary. Runs before the access
    // gate. Compiled out (and never enforced) unless the `zone` feature is on.
    #[cfg(feature = "zone")]
    {
        let req_zone = crate::middleware::zone::request_zone(&state, peer_ip.as_deref(), &headers);
        if page.zone != req_zone {
            return Ok(not_found_response());
        }
    }

    // Access gate (the "who" axis). The interactive path is the browser cookie
    // set by the unlock form; the non-interactive path is HTTP Basic Auth
    // carrying the same page password, so headless agents (curl, an LLM tool)
    // can fetch a protected `.md`/`.html` with just "URL + password" — no cookie
    // dance. Same secret, same strength, over HTTPS; the gate only re-renders
    // when neither path checks out.
    match page.access {
        Access::Noauth => {}
        Access::Password => {
            if !cookie_valid(&state, &uuid, &jar) && !basic_auth_valid(&page, &headers).await {
                return Ok(render_gate(&uuid, &rel_path, false));
            }
        }
    }

    serve_file(&state, &page, &rel_path, &real_ip, &headers, owner.0.is_some()).await
}

async fn serve_file(
    state: &AppState,
    page: &Page,
    raw_path: &str,
    real_ip: &RealIp,
    headers: &HeaderMap,
    is_admin: bool,
) -> Result<Response, ServerError> {
    let mut rel = raw_path.trim_start_matches('/').to_string();
    if rel.is_empty() {
        rel = "index.html".to_string();
    }
    // Defense in depth — DiskStorage also rejects `..`, but bouncing here is
    // both faster and clearer in logs.
    if rel.split('/').any(|seg| seg == ".." || seg == ".") {
        return Ok(not_found_response());
    }

    let (bytes, served_path, status) = match state.storage.read(&page.uuid, &rel).await? {
        Some(b) => (b, rel.clone(), 200u16),
        None => {
            if page.mode == Mode::Spa {
                match state.storage.read(&page.uuid, "index.html").await? {
                    Some(b) => (b, "index.html".to_string(), 200u16),
                    None => {
                        record_hit_async(
                            state.clone(),
                            page.uuid.clone(),
                            rel.clone(),
                            real_ip,
                            headers,
                            404,
                        );
                        return Ok(not_found_response());
                    }
                }
            } else {
                record_hit_async(
                    state.clone(),
                    page.uuid.clone(),
                    rel.clone(),
                    real_ip,
                    headers,
                    404,
                );
                return Ok(not_found_response());
            }
        }
    };

    let mime = mime_guess::from_path(&served_path)
        .first_or_octet_stream()
        .to_string();

    record_hit_async(
        state.clone(),
        page.uuid.clone(),
        rel,
        real_ip,
        headers,
        status,
    );

    // When comments are enabled, inject the widget <script> tag at request
    // time. The bundle on disk is never modified — toggling comments_enabled
    // off makes the very next request serve the original bytes verbatim.
    // We only touch HTML responses; binary assets pass through unchanged.
    let body_bytes = if page.comments_enabled && is_html_mime(&mime) {
        let admin_token = if is_admin {
            mint_access_token(&state.hmac_key, "owner", &format!("admin:{}", page.uuid), 300)
                .ok()
                .map(|(tok, _)| tok)
        } else {
            None
        };
        inject_comments_widget(&bytes, &page.uuid, admin_token.as_deref())
    } else {
        bytes
    };

    let mut resp = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, ensure_text_charset(mime))
        .body(Body::from(body_bytes))
        .map_err(|e| ServerError::Internal(anyhow::anyhow!("build response: {e}")))?;
    // CSP is set per-response, gated by the page's `csp` knob: `Strict` (the
    // default) emits the locked-down policy; `Off` lets the owner declare
    // their own via `<meta http-equiv>` (necessary for sites loading CDN
    // assets — see migration 0003).
    if page.csp == Csp::Strict {
        resp.headers_mut().insert(
            "content-security-policy",
            HeaderValue::from_static(crate::middleware::headers::PAGE_CSP),
        );
    }
    Ok(resp)
}

fn record_hit_async(
    state: AppState,
    page_uuid: String,
    path: String,
    real_ip: &RealIp,
    headers: &HeaderMap,
    status: u16,
) {
    let ip_hash = hmac_ip(&state, &real_ip.0);
    let ua_hash = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| hmac_value(&state, ua));
    let referrer = headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .map(|r| {
            let mut s = r.to_string();
            s.truncate(256);
            s
        });
    let ts = unix_now();

    tokio::spawn(async move {
        let hit = NewHit {
            page_uuid,
            ts,
            ip_hash,
            ua_hash,
            path,
            referrer,
            status: status as i32,
        };
        if let Err(e) = state.repo.record_hit(hit).await {
            tracing::warn!(error = %e, "failed to record hit");
        }
    });
}

async fn submit_gate(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    jar: CookieJar,
    Form(form): Form<GateForm>,
) -> Response {
    match submit_gate_inner(state, uuid, jar, form).await {
        Ok(resp) => resp,
        Err(e) => e.into_response(),
    }
}

async fn submit_gate_inner(
    state: AppState,
    uuid: String,
    jar: CookieJar,
    form: GateForm,
) -> Result<Response, ServerError> {
    let page = match state.repo.find_page(&uuid).await? {
        Some(p) => p,
        None => return Ok(not_found_response()),
    };
    if page.access != Access::Password {
        return Ok(not_found_response());
    }
    let Some(hash) = page.password_hash.as_deref() else {
        return Ok(not_found_response());
    };

    let ok = tokio::task::spawn_blocking({
        let h = hash.to_string();
        let p = form.password.clone();
        move || verify_password(&h, &p).unwrap_or(false)
    })
    .await
    .unwrap_or(false);

    if !ok {
        // Re-render the gate; same status as a normal GET (200) so we don't
        // leak whether the password was wrong vs. anything else.
        let next_path = sanitize_next(&form.next);
        return Ok(render_gate(&uuid, next_path.trim_start_matches('/'), true));
    }

    let expires = unix_now() + PASSWORD_COOKIE_TTL_SECS;
    let cookie_value = make_cookie_value(&state, &uuid, expires);
    let cookie_name = format!("{COOKIE_NAME_PREFIX}{uuid}");
    let path = format!("/p/{uuid}/");
    let mut cookie = Cookie::new(cookie_name, cookie_value);
    cookie.set_path(path.clone());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_secure(!state.config.server.dev);
    cookie.set_max_age(time::Duration::seconds(PASSWORD_COOKIE_TTL_SECS));

    let jar = jar.add(cookie);

    let next = sanitize_next(&form.next);
    let location = format!("/p/{uuid}/{}", next.trim_start_matches('/'));
    let mut resp = Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, location)
        .body(Body::empty())
        .map_err(|e| ServerError::Internal(anyhow::anyhow!("build redirect: {e}")))?;
    // Copy the cookie jar's Set-Cookie headers onto the redirect response.
    for (k, v) in jar.into_response().headers() {
        if k == header::SET_COOKIE {
            resp.headers_mut().append(k, v.clone());
        }
    }
    Ok(resp)
}

fn render_gate(uuid: &str, next_path: &str, error: bool) -> Response {
    let post_action = format!("/p/{uuid}/__gate");
    let tmpl = PasswordGateTemplate {
        post_action: &post_action,
        next_path,
        error,
    };
    match tmpl.render() {
        Ok(body) => {
            let mut resp = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(body))
                .unwrap();
            resp.headers_mut().insert(
                "content-security-policy",
                HeaderValue::from_static(crate::middleware::headers::PAGE_CSP),
            );
            resp
        }
        Err(e) => {
            tracing::error!(error = %e, "render password_gate");
            not_found_response()
        }
    }
}

fn cookie_valid(state: &AppState, uuid: &str, jar: &CookieJar) -> bool {
    let name = format!("{COOKIE_NAME_PREFIX}{uuid}");
    let Some(cookie) = jar.get(&name) else {
        return false;
    };
    let value = cookie.value();
    let Some((expires_str, sig)) = value.split_once('.') else {
        return false;
    };
    let Ok(expires) = expires_str.parse::<i64>() else {
        return false;
    };
    if expires < unix_now() {
        return false;
    }

    let mut mac = HmacSha256::new_from_slice(state.hmac_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(b"page-cookie/v1/");
    mac.update(uuid.as_bytes());
    mac.update(b"|");
    mac.update(expires_str.as_bytes());
    let Ok(sig_bytes) = hex::decode(sig) else {
        return false;
    };
    mac.verify_slice(&sig_bytes).is_ok()
}

/// Non-interactive unlock: accept the page password via HTTP Basic Auth
/// (`Authorization: Basic base64(<uuid>:<password>)`). pipa pages have no
/// per-user accounts — only a shared page secret — so the username is the page
/// UUID by convention (it's already public in the URL, so it adds no second
/// secret, just self-describing scope). An empty username (`:<password>`) is
/// also accepted; a non-empty username that names a different page is rejected.
/// Verifying against the stored Argon2id hash is CPU-heavy, so we only reach for
/// it when a Basic header is actually present (a cookie-bearing browser never
/// gets here).
async fn basic_auth_valid(page: &Page, headers: &HeaderMap) -> bool {
    if page.access != Access::Password {
        return false;
    }
    let Some(hash) = page.password_hash.as_deref() else {
        return false;
    };
    let Some(raw) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    // Case-insensitive "Basic " scheme prefix, then base64(user:pass).
    let Some(encoded) = raw
        .strip_prefix("Basic ")
        .or_else(|| raw.strip_prefix("basic "))
    else {
        return false;
    };
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let Ok(creds) = String::from_utf8(decoded) else {
        return false;
    };
    // Split on the first ':' — passwords may contain ':', usernames can't.
    let Some((user, password)) = creds.split_once(':') else {
        return false;
    };
    // The page UUID is the conventional username (it's already public in the
    // URL, so it carries no secret — just self-describing scope). Reject a
    // non-empty username that names a different page so a credential meant for
    // page A can't be aimed at page B by mistake; the bare `:password` form
    // (empty username) stays valid for clients that only carry the secret.
    if !user.is_empty() && user != page.uuid {
        return false;
    }
    if password.is_empty() {
        return false;
    }
    let password = password.to_string();

    let h = hash.to_string();
    tokio::task::spawn_blocking(move || verify_password(&h, &password).unwrap_or(false))
        .await
        .unwrap_or(false)
}

fn make_cookie_value(state: &AppState, uuid: &str, expires: i64) -> String {
    let expires_str = expires.to_string();
    let mut mac = HmacSha256::new_from_slice(state.hmac_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(b"page-cookie/v1/");
    mac.update(uuid.as_bytes());
    mac.update(b"|");
    mac.update(expires_str.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("{expires_str}.{sig}")
}

fn sanitize_next(next: &str) -> String {
    // Only allow rooted-but-not-cross-origin paths. We strip leading slashes
    // before re-prefixing with `/p/<uuid>/` so a malicious `next` can't
    // redirect the user away.
    if next.is_empty() || next.contains("://") || next.starts_with("//") {
        return String::new();
    }
    next.trim_start_matches('/').to_string()
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn not_found_response() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

/// `mime_guess` returns base types like `text/markdown` or `text/plain` with no
/// charset. Browsers then fall back to a locale default and mangle UTF-8 — an
/// accented `.md` served as bare `text/markdown` shows up as mojibake. Append
/// `; charset=utf-8` to every `text/*` type that doesn't already declare one.
/// Binary types (images, octet-stream, …) pass through untouched.
pub(crate) fn ensure_text_charset(mime: String) -> String {
    if mime.starts_with("text/") && !mime.to_ascii_lowercase().contains("charset") {
        format!("{mime}; charset=utf-8")
    } else {
        mime
    }
}

fn is_html_mime(mime: &str) -> bool {
    // mime_guess returns variants like "text/html" and "text/html; charset=…"
    let head = mime.split(';').next().unwrap_or(mime).trim();
    head.eq_ignore_ascii_case("text/html") || head.eq_ignore_ascii_case("application/xhtml+xml")
}

/// Splice the comments widget script tag into an HTML body just before
/// `</body>` (case-insensitive search). If no closing body tag exists we
/// append at the end of the document — small SPA shells without a literal
/// `</body>` still get the widget. Idempotent for the per-request case: we
/// generate the bundle on each response, the stored file is untouched.
fn inject_comments_widget(html: &Bytes, page_uuid: &str, admin_token: Option<&str>) -> Bytes {
    let token_attr = match admin_token {
        Some(tok) => format!(" data-token=\"{tok}\""),
        None => String::new(),
    };
    let snippet = format!(
        "<script src=\"/api/comments/widget.js\" data-page=\"{page_uuid}\"{token_attr} async></script>"
    );
    let Ok(text) = std::str::from_utf8(html) else {
        // Non-UTF8 HTML is exotic enough that we serve it untouched rather
        // than risk corrupting bytes.
        return html.clone();
    };
    let lower = text.to_ascii_lowercase();
    let insert_at = lower.rfind("</body>").unwrap_or(text.len());
    let mut out = String::with_capacity(text.len() + snippet.len());
    out.push_str(&text[..insert_at]);
    out.push_str(&snippet);
    out.push_str(&text[insert_at..]);
    Bytes::from(out)
}
