use async_trait::async_trait;
use bytes::Bytes;

use crate::audit::AuditEvent;
use crate::comment::{Comment, CommentStatus, NewComment};
use crate::device::{Admin, Device, OwnerSession, RefreshToken, Scope, SetupCode, StepUpToken};
use crate::error::Result;
use crate::hit::NewHit;
use crate::page::{NewPage, Page, PageStats};

#[derive(Debug, Clone)]
pub struct StagingHandle {
    pub id: String,
}

#[derive(Debug, Clone, Copy)]
pub struct PromotedInfo {
    pub size_bytes: u64,
    pub file_count: u64,
}

#[async_trait]
pub trait Storage: Send + Sync {
    async fn begin_staging(&self) -> Result<StagingHandle>;
    async fn put_staged(&self, h: &StagingHandle, rel_path: &str, bytes: Bytes) -> Result<()>;
    /// Atomically swap staging → `pages/<uuid>/`. Old version is moved to trash.
    async fn promote(&self, h: StagingHandle, page_uuid: &str) -> Result<PromotedInfo>;
    async fn read(&self, page_uuid: &str, rel_path: &str) -> Result<Option<Bytes>>;
    async fn delete_page(&self, page_uuid: &str) -> Result<()>;
}

#[async_trait]
pub trait Repository: Send + Sync {
    // pages
    async fn create_page(&self, p: NewPage) -> Result<Page>;
    async fn update_page(&self, p: Page) -> Result<Page>;
    async fn find_page(&self, uuid: &str) -> Result<Option<Page>>;
    async fn list_pages(&self, owner_kind: &str, owner_id: &str) -> Result<Vec<Page>>;
    async fn delete_page(&self, uuid: &str) -> Result<()>;

    // hits
    async fn record_hit(&self, h: NewHit) -> Result<()>;
    async fn stats(&self, page_uuid: &str, since_ts: i64) -> Result<PageStats>;

    // comments
    async fn enable_comments(
        &self,
        page_uuid: &str,
        enabled: bool,
        require_approval: bool,
    ) -> Result<()>;
    async fn create_comment(&self, c: NewComment) -> Result<Comment>;
    async fn list_comments(&self, page_uuid: &str, include_hidden: bool) -> Result<Vec<Comment>>;
    async fn find_comment(&self, id: &str) -> Result<Option<Comment>>;
    async fn set_comment_status(&self, id: &str, status: CommentStatus) -> Result<()>;
    async fn delete_comment(&self, id: &str) -> Result<()>;
    async fn count_recent_comments(
        &self,
        page_uuid: &str,
        ip_hash: &str,
        since_ts: i64,
    ) -> Result<u64>;
    async fn count_recent_comments_server(&self, ip_hash: &str, since_ts: i64) -> Result<u64>;

    // audit
    async fn record_audit(&self, e: AuditEvent) -> Result<()>;
    async fn recent_audit(&self, since_ts: i64) -> Result<Vec<AuditEvent>>;
}

#[derive(Debug, Clone)]
pub struct RefreshTokenIssued {
    pub device: Device,
    pub refresh_plaintext: String,
    pub refresh: RefreshToken,
}

#[derive(Debug, Clone)]
pub enum PollResult {
    Pending,
    Approved(RefreshTokenIssued),
    Expired,
}

#[async_trait]
pub trait AuthStore: Send + Sync {
    // setup codes
    async fn issue_setup_code(&self) -> Result<SetupCode>;
    async fn consume_setup_code(&self, code: &str) -> Result<bool>;
    async fn devices_count(&self) -> Result<u64>;

    // devices
    async fn create_device(&self, label: &str, scope: Scope) -> Result<Device>;
    async fn list_devices(&self) -> Result<Vec<Device>>;
    async fn revoke_device(&self, id: &str) -> Result<()>;
    async fn touch_device(&self, id: &str) -> Result<()>;

    // refresh tokens
    async fn issue_refresh(
        &self,
        device_id: &str,
        scope: Scope,
        ttl_seconds: i64,
    ) -> Result<(RefreshToken, String)>;
    async fn rotate_refresh(&self, plaintext: &str) -> Result<(RefreshToken, String)>;
    async fn revoke_refresh(&self, plaintext: &str) -> Result<()>;
    async fn lookup_refresh(&self, plaintext: &str) -> Result<Option<(RefreshToken, Device)>>;

    // device pairings (device flow)
    async fn begin_pairing(&self) -> Result<(String, String)>;
    async fn approve_pairing(
        &self,
        code: &str,
        label: &str,
        scope: Scope,
    ) -> Result<RefreshTokenIssued>;
    async fn poll_pairing(&self, code: &str, secret: &str) -> Result<PollResult>;

    // step-up
    async fn begin_step_up(
        &self,
        device_id: &str,
        operation: &str,
        target: Option<&str>,
        ip_hash: Option<&str>,
    ) -> Result<StepUpToken>;
    async fn confirm_step_up(&self, code: &str) -> Result<()>;
    async fn consume_step_up(
        &self,
        code: &str,
        device_id: &str,
        operation: &str,
        target: Option<&str>,
    ) -> Result<bool>;
    /// Non-mutating peek: returns one of
    /// `"pending" | "confirmed" | "consumed" | "expired"` or `"unknown"` when
    /// the code does not exist. Used by the polling status endpoint so the
    /// CLI can wait without observing partial state via mutating calls.
    async fn step_up_observe(&self, code: &str) -> Result<&'static str>;

    /// Fetch a single step-up token by code. Used by the confirmation page
    /// to render the operation/target/device summary before the user clicks
    /// "confirm". Returns `Ok(None)` for unknown codes.
    async fn step_up_get(&self, code: &str) -> Result<Option<StepUpToken>>;

    /// Fetch a single device by id. Used by the confirmation page to render
    /// the requesting device's human label.
    async fn get_device(&self, id: &str) -> Result<Option<Device>>;

    // owner sessions (browser-side claim of server ownership)
    async fn owner_sessions_count(&self) -> Result<u64>;
    async fn create_owner_session(
        &self,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> Result<OwnerSession>;
    async fn find_owner_session(&self, id: &str) -> Result<Option<OwnerSession>>;
    async fn touch_owner_session(&self, id: &str) -> Result<()>;
    async fn list_owner_sessions(&self) -> Result<Vec<OwnerSession>>;
    async fn revoke_owner_session(&self, id: &str) -> Result<()>;

    // admin user (single-row, Phase 1 single-owner)
    async fn count_admins(&self) -> Result<u64>;
    /// Create the admin row + its synthetic "Admin Web UI" device in one
    /// transaction. Fails with `AlreadyExists` when an admin already exists.
    async fn create_admin(&self, username: &str, password_hash: &str) -> Result<Admin>;
    async fn find_admin_by_username(&self, username: &str) -> Result<Option<Admin>>;
    async fn get_admin(&self) -> Result<Option<Admin>>;
    /// Delete the admin row + revoke its synthetic device. Used by
    /// `gapes-server reset-claim` to allow a fresh `/setup` to create a new
    /// admin from scratch.
    async fn delete_admin(&self) -> Result<()>;
}
