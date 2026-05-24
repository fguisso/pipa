//! `GET <ui_path>/activity` — full server-rendered audit log (last 200).
//! Also provides `GET /api/audit/recent?since=&limit=` — a minimal JSON
//! endpoint reused by the dashboard's live refresh.
//!
//! Authorization: `manage:devices` is the closest existing scope to "owner
//! looking at their own audit log" (it already gates device listing, which
//! is itself audit-y). We keep the read path narrow so a `read:*` token can
//! list pages without also exposing the audit log to automation.

use askama::Template;
use axum::Json;
use axum::extract::{Query, State};
use axum::response::Response;
use gapes_core::audit::AuditEvent;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthClaims, check_scope};
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::dashboard::render;
use super::session::{AdminSession, ui_path};

const HARD_LIMIT: usize = 500;
const DEFAULT_LIMIT: usize = 200;

#[derive(Template)]
#[template(path = "admin/activity.html")]
struct ActivityTemplate<'a> {
    ui_path: &'a str,
    show_nav: bool,
    events: Vec<ActivityRow>,
}

pub struct ActivityRow {
    pub ts_human: String,
    pub action: String,
    pub actor: String,
    pub target: Option<String>,
    pub scope: Option<String>,
    pub success: bool,
    pub details: Option<String>,
}

pub async fn activity_page(
    State(state): State<AppState>,
    _session: AdminSession,
) -> Response {
    let rows = state
        .repo
        .recent_audit(0)
        .await
        .unwrap_or_default();
    let mut rows = rows;
    rows.sort_by(|a, b| b.ts.cmp(&a.ts));
    rows.truncate(DEFAULT_LIMIT);
    let events: Vec<ActivityRow> = rows
        .iter()
        .map(|e| ActivityRow {
            ts_human: fmt_ts(e.ts),
            action: e.action.as_str().to_string(),
            actor: e.actor.clone(),
            target: e.target.clone(),
            scope: e.scope.clone(),
            success: e.success,
            details: e.details.clone(),
        })
        .collect();

    let tmpl = ActivityTemplate {
        ui_path: ui_path(&state),
        show_nav: true,
        events,
    };
    render(tmpl)
}

#[derive(Debug, Deserialize)]
pub struct RecentQuery {
    #[serde(default)]
    pub since: Option<i64>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct RecentEvent {
    pub ts: i64,
    pub action: &'static str,
    pub actor: String,
    pub target: Option<String>,
    pub scope: Option<String>,
    pub success: bool,
    pub details: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RecentResponse {
    pub events: Vec<RecentEvent>,
}

pub async fn recent_audit_json(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Query(q): Query<RecentQuery>,
) -> Result<Json<RecentResponse>, ServerError> {
    if !check_scope(&claims, "manage", Some("devices")) {
        return Err(ApiError::forbidden(
            "insufficient_scope",
            "manage:devices scope required",
        )
        .into());
    }
    let since = q.since.unwrap_or(0).max(0);
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(HARD_LIMIT);
    let mut rows = state.repo.recent_audit(since).await?;
    rows.sort_by(|a, b| b.ts.cmp(&a.ts));
    rows.truncate(limit);
    let events = rows.iter().map(to_recent).collect();
    Ok(Json(RecentResponse { events }))
}

fn to_recent(e: &AuditEvent) -> RecentEvent {
    RecentEvent {
        ts: e.ts,
        action: e.action.as_str(),
        actor: e.actor.clone(),
        target: e.target.clone(),
        scope: e.scope.clone(),
        success: e.success,
        details: e.details.clone(),
    }
}

/// Serialize a slice of audit events into the JSON shape `RecentResponse`
/// produces. Used by the dashboard bootstrap so first paint shares the same
/// shape Alpine will later receive from the API.
pub fn audit_events_to_json(events: &[AuditEvent]) -> String {
    let rows: Vec<RecentEvent> = events.iter().map(to_recent).collect();
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
}

fn fmt_ts(ts: i64) -> String {
    let secs = ts.max(0);
    let dt = time::OffsetDateTime::from_unix_timestamp(secs).ok();
    match dt {
        Some(d) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            d.year(),
            d.month() as u8,
            d.day(),
            d.hour(),
            d.minute()
        ),
        None => ts.to_string(),
    }
}

