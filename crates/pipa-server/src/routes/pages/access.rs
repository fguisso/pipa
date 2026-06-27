//! `POST /api/pages/:uuid/access` — change a page's access method, zone, and/or
//! CSP knob. Loosening security is destructive and drives step-up:
//!   * access → noauth (removing the auth gate)
//!   * zone   → public (exposing the page to the internet)
//! Tightening (→ password, → private) and csp-only edits need just `admin:<uuid>`.
//! Password rotation reuses this endpoint — `access=password` with a fresh
//! `password` field.
//!
//! `zone` is accepted regardless of the `zone` Cargo feature (it's just a
//! stored column); the feature only governs whether the serving layer
//! *enforces* it.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use pipa_adapters::hash_password;
use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::{Access, Csp, Page, Zone};
use serde::Deserialize;

use crate::auth::{AuthClaims, verify_step_up};
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::util::{PageView, access_str, require_admin, require_destroy, unix_now};

const STEP_UP_HEADER: &str = "x-stepup-code";
/// Step-up operation string gating a security loosening (access->noauth or
/// zone->public). Audit logs each changed axis under its own action below.
const ACCESS_OPERATION: &str = "page.weaken_security";

#[derive(Debug, Deserialize)]
pub struct AccessRequest {
    #[serde(default)]
    pub access: Option<String>,
    #[serde(default)]
    pub zone: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// Optional per-page CSP knob. `strict` (default) or `off`. Non-destructive.
    #[serde(default)]
    pub csp: Option<String>,
}

pub async fn change_access(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
    headers: HeaderMap,
    Json(req): Json<AccessRequest>,
) -> Result<axum::response::Response, ServerError> {
    let new_access: Option<Access> = match req.access.as_deref() {
        Some(s) => Some(s.parse().map_err(|_| {
            ApiError::bad_request("invalid_access", "access must be password|noauth")
        })?),
        None => None,
    };
    let new_zone: Option<Zone> = match req.zone.as_deref() {
        Some(s) => Some(s.parse().map_err(|_| {
            ApiError::bad_request("invalid_zone", "zone must be public|private")
        })?),
        None => None,
    };
    let new_csp: Option<Csp> = match req.csp.as_deref() {
        Some(s) => Some(s.parse().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_csp",
                "csp must be strict|off",
            )
        })?),
        None => None,
    };

    if new_access.is_none() && new_zone.is_none() && new_csp.is_none() {
        return Err(ApiError::bad_request(
            "no_fields",
            "at least one of `access`, `zone`, or `csp` must be set",
        )
        .into());
    }

    let page = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;
    let old_access = page.access;
    let old_zone = page.zone;
    let old_csp = page.csp;

    // Authorization. Loosening security (access→noauth, zone→public) is
    // destructive: destroy scope + step-up. Everything else is admin-only.
    let loosening =
        matches!(new_access, Some(Access::Noauth)) || matches!(new_zone, Some(Zone::Public));
    if loosening {
        require_destroy(&claims, &uuid)?;
        let code = headers
            .get(STEP_UP_HEADER)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError::forbidden(
                    "step_up_required",
                    "loosening a page's security requires a step-up confirmation",
                )
            })?;
        let ok = verify_step_up(&state, code.trim(), &claims.sub, ACCESS_OPERATION, Some(&uuid))
            .await
            .map_err(ServerError::Internal)?;
        if !ok {
            return Err(ApiError::forbidden(
                "step_up_invalid",
                "step-up code missing, expired, or for a different operation",
            )
            .into());
        }
    } else {
        require_admin(&claims, &uuid)?;
    }

    let mut updated: Page = page.clone();
    updated.updated_at = unix_now();
    if let Some(z) = new_zone {
        updated.zone = z;
    }
    if let Some(c) = new_csp {
        updated.csp = c;
    }

    // Password handling tracks the access method. Moving to `password` needs a
    // secret (or keeps the existing hash if already password-protected and none
    // is re-supplied); moving to `noauth` clears it.
    if let Some(a) = new_access {
        updated.access = a;
        match a {
            Access::Password => match req.password.as_ref().filter(|s| !s.is_empty()) {
                Some(pw) => {
                    let pw = pw.clone();
                    let hash = tokio::task::spawn_blocking(move || hash_password(&pw))
                        .await
                        .map_err(|e| anyhow::anyhow!("argon2 join: {e}"))?
                        .map_err(ServerError::Internal)?;
                    updated.password_hash = Some(hash);
                }
                None => {
                    if updated.password_hash.is_none() {
                        return Err(ApiError::bad_request(
                            "missing_password",
                            "password field is required when access=password",
                        )
                        .into());
                    }
                }
            },
            Access::Noauth => {
                updated.password_hash = None;
            }
        }
    }

    let saved = state.repo.update_page(updated).await?;

    // Audit each changed axis under its own action, so the log names exactly
    // what changed: access -> `page.access_change`, zone -> `page.zone_change`,
    // csp -> `page.update`. A request that touches several emits one event each.
    let now = unix_now();
    let mut events: Vec<(AuditAction, serde_json::Value)> = Vec::new();
    if let Some(a) = new_access {
        events.push((
            AuditAction::PageAccessChange,
            serde_json::json!({ "from": access_str(old_access), "to": a.as_str() }),
        ));
    }
    if let Some(z) = new_zone {
        events.push((
            AuditAction::PageZoneChange,
            serde_json::json!({ "from": old_zone.as_str(), "to": z.as_str() }),
        ));
    }
    if let Some(c) = new_csp {
        events.push((
            AuditAction::PageUpdate,
            serde_json::json!({ "csp": { "from": old_csp.as_str(), "to": c.as_str() } }),
        ));
    }
    for (action, details) in events {
        let _ = state
            .repo
            .record_audit(
                AuditEvent::success(now, claims.sub.clone(), action)
                    .with_target(uuid.clone())
                    .with_scope(claims.scope.clone())
                    .with_details(details.to_string()),
            )
            .await;
    }

    Ok((axum::http::StatusCode::OK, Json(PageView::from(&saved))).into_response())
}
