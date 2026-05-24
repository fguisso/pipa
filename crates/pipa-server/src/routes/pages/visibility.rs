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
use pipa_core::{Page, Visibility};
use serde::Deserialize;

use crate::auth::{AuthClaims, verify_step_up};
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::util::{PageView, require_admin, require_destroy, unix_now, vis_str};

const STEP_UP_HEADER: &str = "x-stepup-code";
const VISIBILITY_OPERATION: &str = "page.visibility_change";

#[derive(Debug, Deserialize)]
pub struct VisibilityRequest {
    pub visibility: String,
    #[serde(default)]
    pub password: Option<String>,
}

pub async fn change_visibility(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
    headers: HeaderMap,
    Json(req): Json<VisibilityRequest>,
) -> Result<axum::response::Response, ServerError> {
    let new_vis: Visibility = req.visibility.parse().map_err(|_| {
        ApiError::bad_request(
            "invalid_visibility",
            "visibility must be private|public|password",
        )
    })?;

    let page = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;
    let old_vis = page.visibility;

    // Authorization differs by the new (target) visibility:
    //   public   → destroy:<uuid> + step-up (matches DELETE in destructiveness)
    //   private  → admin:<uuid>
    //   password → admin:<uuid> (and we hash + replace)
    match new_vis {
        Visibility::Public => {
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
        Visibility::Private | Visibility::Password => {
            require_admin(&claims, &uuid)?;
        }
    }

    let mut updated: Page = page.clone();
    updated.visibility = new_vis;
    updated.updated_at = unix_now();

    match new_vis {
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

    let saved = state.repo.update_page(updated).await?;

    let details = serde_json::json!({
        "from": vis_str(old_vis),
        "to": vis_str(new_vis),
    })
    .to_string();
    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(
                unix_now(),
                claims.sub.clone(),
                AuditAction::PageVisibilityChange,
            )
            .with_target(uuid.clone())
            .with_scope(claims.scope.clone())
            .with_details(details),
        )
        .await;

    Ok((axum::http::StatusCode::OK, Json(PageView::from(&saved))).into_response())
}
