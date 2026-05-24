use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::Router;
use axum::body::Body;
use axum::extract::{Form, Path, State};
use axum::http::header::{HeaderMap, HeaderValue};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use pipa_adapters::verify_password;
use pipa_core::{Mode, NewHit, Page, Visibility};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::error::ServerError;
use crate::ip_hash::{hmac_ip, hmac_value};
use crate::middleware::forwarded::RealIp;
use crate::middleware::headers::PageCspLayer;
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
    Router::new()
        .route("/p/:uuid/__gate", post(submit_gate))
        .route("/p/:uuid", get(serve_root))
        .route("/p/:uuid/", get(serve_root))
        .route("/p/:uuid/*path", get(serve_path))
        .layer(PageCspLayer)
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
    headers: HeaderMap,
    jar: CookieJar,
) -> Response {
    serve(state, uuid, String::new(), real_ip, headers, jar).await
}

async fn serve_path(
    State(state): State<AppState>,
    Path((uuid, path)): Path<(String, String)>,
    real_ip: RealIp,
    headers: HeaderMap,
    jar: CookieJar,
) -> Response {
    serve(state, uuid, path, real_ip, headers, jar).await
}

async fn serve(
    state: AppState,
    uuid: String,
    rel_path: String,
    real_ip: RealIp,
    headers: HeaderMap,
    jar: CookieJar,
) -> Response {
    match serve_inner(state, uuid, rel_path, real_ip, headers, jar).await {
        Ok(resp) => resp,
        Err(e) => e.into_response(),
    }
}

async fn serve_inner(
    state: AppState,
    uuid: String,
    rel_path: String,
    real_ip: RealIp,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<Response, ServerError> {
    let page = match state.repo.find_page(&uuid).await? {
        Some(p) => p,
        None => return Ok(not_found_response()),
    };

    match page.visibility {
        // TODO(phase-1-auth.md §sessions): once owner sessions are speced,
        // allow the owner cookie to browse private pages here. For now we
        // always 404 to avoid leaking existence.
        Visibility::Private => return Ok(not_found_response()),
        Visibility::Public => {}
        Visibility::Password => {
            if !cookie_valid(&state, &uuid, &jar) {
                return Ok(render_gate(&uuid, &rel_path, false));
            }
        }
    }

    serve_file(&state, &page, &rel_path, &real_ip, &headers).await
}

async fn serve_file(
    state: &AppState,
    page: &Page,
    raw_path: &str,
    real_ip: &RealIp,
    headers: &HeaderMap,
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

    let mut resp = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(bytes))
        .map_err(|e| ServerError::Internal(anyhow::anyhow!("build response: {e}")))?;
    // PageCspLayer adds CSP for the whole router, but we double-set here so
    // an accidental layer removal still leaves the page hardened.
    resp.headers_mut().insert(
        "content-security-policy",
        HeaderValue::from_static(crate::middleware::headers::PAGE_CSP),
    );
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
    if page.visibility != Visibility::Password {
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
