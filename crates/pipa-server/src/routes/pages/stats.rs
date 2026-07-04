//! `GET /api/pages/:uuid/stats?range=24h|7d|30d|all` — aggregated analytics
//! over the requested window. Returns `views`, `uniques`, `top_paths`,
//! `top_referrers` plus the resolved `since_ts` so the CLI can echo "last 7
//! days starting at <ts>" without re-computing.

use axum::Json;
use axum::extract::{Path, Query, State};
use pipa_core::PageStats;
use serde::{Deserialize, Serialize};

use crate::auth::AuthClaims;
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::util::{caller_identity, require_page_access, require_read, unix_now};

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    #[serde(default)]
    pub range: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub range: String,
    pub since_ts: i64,
    pub stats: PageStats,
}

pub async fn stats(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<StatsResponse>, ServerError> {
    require_read(&claims, &uuid)?;

    let page = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;
    let caller = caller_identity(&state, &claims).await;
    require_page_access(&state, &caller, &page, false).await?;

    let range = q.range.unwrap_or_else(|| "7d".into());
    let since_ts = since_for(&range)?;
    let stats = state.repo.stats(&uuid, since_ts).await?;
    Ok(Json(StatsResponse {
        range,
        since_ts,
        stats,
    }))
}

fn since_for(range: &str) -> Result<i64, ApiError> {
    let now = unix_now();
    match range {
        "24h" => Ok(now - 24 * 3600),
        "7d" => Ok(now - 7 * 24 * 3600),
        "30d" => Ok(now - 30 * 24 * 3600),
        "all" => Ok(0),
        other => Err(ApiError::bad_request(
            "invalid_range",
            format!("unknown range {other:?}; expected 24h|7d|30d|all"),
        )),
    }
}
