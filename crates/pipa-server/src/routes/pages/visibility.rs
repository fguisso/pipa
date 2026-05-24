//! `POST /api/pages/:uuid/visibility` — set a page's visibility. Going
//! `→ public` is destructive (the page becomes world-readable) and requires
//! step-up + `destroy:<uuid>`. `→ private` and `→ password` only need
//! `admin:<uuid>` and never require step-up. Password rotation reuses this
//! same endpoint — `visibility=password` with a fresh `password` field.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use pipa_adapters::hash_password;
use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::{Csp, Page, Visibility};
use serde::Deserialize;

use crate::auth::{AuthClaims, verify_step_up};
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::util::{PageView, require_admin, require_destroy, unix_now, vis_str};

const STEP_UP_HEADER: &str = "x-stepup-code";
const VISIBILITY_OPERATION: &str = "page.visibility_change";

#[derive(Debug, Deserialize)]
pub struct VisibilityRequest {
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// Optional per-page CSP knob. `strict` (default) or `off`. Changing csp
    /// is *not* destructive — the owner can always flip it back — so we don't
    /// require step-up for this field, only `admin:<uuid>`.
    #[serde(default)]
    pub csp: Option<String>,
}

pub async fn change_visibility(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
    headers: HeaderMap,
    Json(req): Json<VisibilityRequest>,
) -> Result<axum::response::Response, ServerError> {
    // Parse optional knobs up front so we can reject bad input before doing
    // any DB work.
    let new_vis: Option<Visibility> = match req.visibility.as_deref() {
        Some(s) => Some(s.parse().map_err(|_| {
            ApiError::bad_request(
                "invalid_visibility",
                "visibility must be private|public|password",
            )
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

    if new_vis.is_none() && new_csp.is_none() {
        return Err(ApiError::bad_request(
            "no_fields",
            "at least one of `visibility` or `csp` must be set",
        )
        .into());
    }

    let page = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;
    let old_vis = page.visibility;
    let old_csp = page.csp;

    // Authorization. If visibility is changing, the new (target) visibility
    // dictates: `public` needs destroy + step-up (matches DELETE), the others
    // just need admin. A csp-only edit is non-destructive, so admin is enough.
    match new_vis {
        Some(Visibility::Public) => {
            require_destroy(&claims, &uuid)?;
            let code = headers
                .get(STEP_UP_HEADER)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    ApiError::forbidden(
                        "step_up_required",
                        "making a page public requires a step-up confirmation",
                    )
                })?;
            let ok = verify_step_up(
                &state,
                code.trim(),
                &claims.sub,
                VISIBILITY_OPERATION,
                Some(&uuid),
            )
            .await
            .map_err(ServerError::Internal)?;
            if !ok {
                return Err(ApiError::forbidden(
                    "step_up_invalid",
                    "step-up code missing, expired, or for a different operation",
                )
                .into());
            }
        }
        Some(Visibility::Private) | Some(Visibility::Password) | None => {
            require_admin(&claims, &uuid)?;
        }
    }

    let mut updated: Page = page.clone();
    updated.updated_at = unix_now();
    if let Some(v) = new_vis {
        updated.visibility = v;
    }
    if let Some(c) = new_csp {
        updated.csp = c;
    }

    // Password handling only matters when visibility is being changed *to*
    // password (or stays password but with a new secret). If visibility isn't
    // being touched, leave password_hash alone.
    if let Some(v) = new_vis {
        match v {
            Visibility::Password => {
                let Some(pw) = req.password.as_ref().filter(|s| !s.is_empty()) else {
                    return Err(ApiError::bad_request(
                        "missing_password",
                        "password field is required when visibility=password",
                    )
                    .into());
                };
                let pw = pw.clone();
                let hash = tokio::task::spawn_blocking(move || hash_password(&pw))
                    .await
                    .map_err(|e| anyhow::anyhow!("argon2 join: {e}"))?
                    .map_err(ServerError::Internal)?;
                updated.password_hash = Some(hash);
            }
            _ => {
                updated.password_hash = None;
            }
        }
    }

    let saved = state.repo.update_page(updated).await?;

    // Audit `page.visibility_change` if visibility moved; otherwise `page.update`.
    let (action, details) = if let Some(v) = new_vis {
        let mut d = serde_json::Map::new();
        d.insert("from".into(), serde_json::Value::String(vis_str(old_vis).into()));
        d.insert("to".into(), serde_json::Value::String(vis_str(v).into()));
        if let Some(c) = new_csp {
            d.insert(
                "csp".into(),
                serde_json::json!({ "from": old_csp.as_str(), "to": c.as_str() }),
            );
        }
        (AuditAction::PageVisibilityChange, serde_json::Value::Object(d).to_string())
    } else {
        // csp-only path
        let c = new_csp.expect("must be Some — bailed earlier otherwise");
        let d = serde_json::json!({
            "csp": { "from": old_csp.as_str(), "to": c.as_str() }
        });
        (AuditAction::PageUpdate, d.to_string())
    };
    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), claims.sub.clone(), action)
                .with_target(uuid.clone())
                .with_scope(claims.scope.clone())
                .with_details(details),
        )
        .await;

    Ok((axum::http::StatusCode::OK, Json(PageView::from(&saved))).into_response())
}
