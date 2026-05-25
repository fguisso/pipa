use std::str::FromStr;

use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::comment::{Comment, CommentStatus};
use pipa_core::device::{
    Admin, Device, DevicePairing, OwnerSession, RefreshToken, Scope, SetupCode, StepUpToken,
};
use pipa_core::error::{CoreError, Result};
use pipa_core::page::{Csp, Mode, Page, Visibility};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

pub fn page_from_row(row: &SqliteRow) -> Result<Page> {
    let mode_s: String = row
        .try_get("mode")
        .map_err(|e| CoreError::RepositoryFailure(format!("page.mode: {e}")))?;
    let visibility_s: String = row
        .try_get("visibility")
        .map_err(|e| CoreError::RepositoryFailure(format!("page.visibility: {e}")))?;
    let csp_s: String = row
        .try_get("csp")
        .map_err(|e| CoreError::RepositoryFailure(format!("page.csp: {e}")))?;
    Ok(Page {
        uuid: get(row, "uuid")?,
        name: opt(row, "name")?,
        mode: Mode::from_str(&mode_s)?,
        visibility: Visibility::from_str(&visibility_s)?,
        password_hash: opt(row, "password_hash")?,
        owner_kind: get(row, "owner_kind")?,
        owner_id: get(row, "owner_id")?,
        size_bytes: get_i64(row, "size_bytes")? as u64,
        file_count: get_i64(row, "file_count")? as u64,
        comments_enabled: get_i64(row, "comments_enabled")? != 0,
        comments_require_approval: get_i64(row, "comments_require_approval")? != 0,
        csp: Csp::from_str(&csp_s)?,
        archived: get_i64(row, "archived")? != 0,
        created_at: get_i64(row, "created_at")?,
        updated_at: get_i64(row, "updated_at")?,
    })
}

pub fn comment_from_row(row: &SqliteRow) -> Result<Comment> {
    let status_s: String = row
        .try_get("status")
        .map_err(|e| CoreError::RepositoryFailure(format!("comment.status: {e}")))?;
    Ok(Comment {
        id: get(row, "id")?,
        page_uuid: get(row, "page_uuid")?,
        author: get(row, "author")?,
        body_md: get(row, "body_md")?,
        body_html: get(row, "body_html")?,
        contact: opt(row, "contact")?,
        ts: get_i64(row, "ts")?,
        ip_hash: get(row, "ip_hash")?,
        status: CommentStatus::from_str(&status_s)?,
        user_agent: opt(row, "user_agent")?,
        anchor_selector: get(row, "anchor_selector")?,
        anchor_text: get(row, "anchor_text")?,
        anchor_offset: get_i64(row, "anchor_offset")?,
    })
}

pub fn audit_from_row(row: &SqliteRow) -> Result<AuditEvent> {
    let action_s: String = row
        .try_get("action")
        .map_err(|e| CoreError::RepositoryFailure(format!("audit.action: {e}")))?;
    Ok(AuditEvent {
        id: row.try_get::<i64, _>("id").ok(),
        ts: get_i64(row, "ts")?,
        actor: get(row, "actor")?,
        ip_hash: opt(row, "ip_hash")?,
        scope: opt(row, "scope")?,
        action: AuditAction::from_str(&action_s)?,
        target: opt(row, "target")?,
        success: get_i64(row, "success")? != 0,
        details: opt(row, "details")?,
    })
}

pub fn device_from_row(row: &SqliteRow) -> Result<Device> {
    let scope_s: String = row
        .try_get("scope")
        .map_err(|e| CoreError::RepositoryFailure(format!("device.scope: {e}")))?;
    Ok(Device {
        id: get(row, "id")?,
        label: get(row, "label")?,
        scope: Scope::from_str(&scope_s)?,
        created_at: get_i64(row, "created_at")?,
        last_seen_at: opt_i64(row, "last_seen_at")?,
        revoked_at: opt_i64(row, "revoked_at")?,
    })
}

pub fn refresh_from_row(row: &SqliteRow) -> Result<RefreshToken> {
    let scope_s: String = row
        .try_get("scope")
        .map_err(|e| CoreError::RepositoryFailure(format!("refresh.scope: {e}")))?;
    Ok(RefreshToken {
        id: get(row, "id")?,
        device_id: get(row, "device_id")?,
        token_hash: get(row, "token_hash")?,
        scope: Scope::from_str(&scope_s)?,
        created_at: get_i64(row, "created_at")?,
        expires_at: get_i64(row, "expires_at")?,
        rotated_to: opt(row, "rotated_to")?,
        revoked_at: opt_i64(row, "revoked_at")?,
    })
}

pub fn pairing_from_row(row: &SqliteRow) -> Result<DevicePairing> {
    Ok(DevicePairing {
        code: get(row, "code")?,
        secret_hash: get(row, "secret_hash")?,
        created_at: get_i64(row, "created_at")?,
        expires_at: get_i64(row, "expires_at")?,
        approved_device_id: opt(row, "approved_device_id")?,
        approved_at: opt_i64(row, "approved_at")?,
        refresh_token_id: opt(row, "refresh_token_id")?,
    })
}

pub fn step_up_from_row(row: &SqliteRow) -> Result<StepUpToken> {
    Ok(StepUpToken {
        code: get(row, "code")?,
        device_id: get(row, "device_id")?,
        operation: get(row, "operation")?,
        target: opt(row, "target")?,
        requesting_ip_hash: opt(row, "requesting_ip_hash")?,
        created_at: get_i64(row, "created_at")?,
        expires_at: get_i64(row, "expires_at")?,
        consumed_at: opt_i64(row, "consumed_at")?,
        confirmed_at: opt_i64(row, "confirmed_at")?,
    })
}

pub fn admin_from_row(row: &SqliteRow) -> Result<Admin> {
    Ok(Admin {
        id: get(row, "id")?,
        username: get(row, "username")?,
        password_hash: get(row, "password_hash")?,
        synthetic_device_id: get(row, "synthetic_device_id")?,
        created_at: get_i64(row, "created_at")?,
    })
}

pub fn owner_session_from_row(row: &SqliteRow) -> Result<OwnerSession> {
    Ok(OwnerSession {
        id: get(row, "id")?,
        created_at: get_i64(row, "created_at")?,
        last_seen_at: opt_i64(row, "last_seen_at")?,
        user_agent: opt(row, "user_agent")?,
        ip: opt(row, "ip")?,
        revoked_at: opt_i64(row, "revoked_at")?,
    })
}

pub fn setup_from_row(row: &SqliteRow) -> Result<SetupCode> {
    Ok(SetupCode {
        code: get(row, "code")?,
        created_at: get_i64(row, "created_at")?,
        expires_at: get_i64(row, "expires_at")?,
        consumed_at: opt_i64(row, "consumed_at")?,
    })
}

fn get(row: &SqliteRow, col: &str) -> Result<String> {
    row.try_get::<String, _>(col)
        .map_err(|e| CoreError::RepositoryFailure(format!("col {col}: {e}")))
}

fn opt(row: &SqliteRow, col: &str) -> Result<Option<String>> {
    row.try_get::<Option<String>, _>(col)
        .map_err(|e| CoreError::RepositoryFailure(format!("col {col}: {e}")))
}

fn get_i64(row: &SqliteRow, col: &str) -> Result<i64> {
    row.try_get::<i64, _>(col)
        .map_err(|e| CoreError::RepositoryFailure(format!("col {col}: {e}")))
}

fn opt_i64(row: &SqliteRow, col: &str) -> Result<Option<i64>> {
    row.try_get::<Option<i64>, _>(col)
        .map_err(|e| CoreError::RepositoryFailure(format!("col {col}: {e}")))
}
