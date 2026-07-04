use async_trait::async_trait;
use bytes::Bytes;

use crate::audit::AuditEvent;
use crate::comment::{Comment, CommentStatus, NewComment};
use crate::device::{Admin, Device, OwnerSession, RefreshToken, Scope, SetupCode, StepUpToken};
use crate::error::Result;
use crate::hit::NewHit;
use crate::page::{NewPage, Page, PageStats};
use crate::user::{NewOAuthIdentity, NewUser, OAuthProvider, User, UserSession};
use crate::workspace::{
    NewWorkspace, Workspace, WorkspaceMemberView, WorkspaceMembership, WorkspaceRole,
};

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
    /// Every page, newest first — for the `local` superuser listing (admin sees
    /// all). Workspace members and users must use the owner-scoped `list_pages`.
    async fn list_all_pages(&self) -> Result<Vec<Page>>;
    /// Count / total bytes of pages owned by (kind, id) — for quota checks.
    async fn count_pages_for_owner(&self, owner_kind: &str, owner_id: &str) -> Result<u64>;
    async fn sum_bytes_for_owner(&self, owner_kind: &str, owner_id: &str) -> Result<u64>;
    /// Re-home a page to a new owner (Phase 4 page transfer).
    async fn transfer_page(&self, uuid: &str, owner_kind: &str, owner_id: &str) -> Result<()>;
    async fn delete_page(&self, uuid: &str) -> Result<()>;
    /// Flip the `archived` flag. Doesn't touch files or visibility; the
    /// serving layer's 404 check is the single source of truth.
    async fn set_page_archived(&self, uuid: &str, archived: bool) -> Result<()>;

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
    /// Create a device. `user_id` links it to a Phase-3 user; `None` is the
    /// Phase-1 single-owner ("local") device.
    async fn create_device(
        &self,
        label: &str,
        scope: Scope,
        user_id: Option<&str>,
    ) -> Result<Device>;
    async fn list_devices(&self) -> Result<Vec<Device>>;
    /// Devices owned by a specific user (for the per-user device UI).
    async fn list_devices_for_user(&self, user_id: &str) -> Result<Vec<Device>>;
    async fn revoke_device(&self, id: &str) -> Result<()>;
    async fn touch_device(&self, id: &str) -> Result<()>;
    /// Associate an existing device with a user after the fact.
    async fn set_device_user(&self, device_id: &str, user_id: &str) -> Result<()>;

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
    /// Approve a pending pairing, minting the device + refresh token. `user_id`
    /// binds the new device to the approving user (Phase 3); `None` when the
    /// single-owner admin approves.
    async fn approve_pairing(
        &self,
        code: &str,
        label: &str,
        scope: Scope,
        user_id: Option<&str>,
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
    /// `pipa-server reset-claim` to allow a fresh `/setup` to create a new
    /// admin from scratch.
    async fn delete_admin(&self) -> Result<()>;

    // users (Phase 3 multi-user)
    /// Create a user AND their personal workspace + owner membership in one
    /// transaction (Phase 4). Fails with `AlreadyExists` on a duplicate username.
    async fn create_user(&self, u: NewUser) -> Result<User>;
    async fn find_user_by_username(&self, username: &str) -> Result<Option<User>>;
    async fn find_user_by_id(&self, id: &str) -> Result<Option<User>>;
    async fn list_users(&self) -> Result<Vec<User>>;
    async fn set_user_disabled(&self, id: &str, disabled: bool) -> Result<()>;

    // user sessions (browser cookie, mirrors owner sessions)
    async fn create_user_session(
        &self,
        user_id: &str,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> Result<UserSession>;
    async fn find_user_session(&self, id: &str) -> Result<Option<UserSession>>;
    async fn touch_user_session(&self, id: &str) -> Result<()>;
    async fn list_user_sessions(&self, user_id: &str) -> Result<Vec<UserSession>>;
    async fn revoke_user_session(&self, id: &str) -> Result<()>;

    // oauth (scaffold — no live provider flow yet)
    async fn link_oauth(&self, ident: NewOAuthIdentity) -> Result<()>;
    async fn find_user_by_oauth(
        &self,
        provider: OAuthProvider,
        subject: &str,
    ) -> Result<Option<User>>;

    // workspaces (Phase 4)
    /// Create a workspace and add `owner_user_id` as its `Owner` in one
    /// transaction.
    async fn create_workspace(&self, ws: NewWorkspace, owner_user_id: &str) -> Result<Workspace>;
    async fn get_workspace(&self, id: &str) -> Result<Option<Workspace>>;
    /// Workspaces the user belongs to, each with the user's role in it.
    async fn list_workspaces_for_user(&self, user_id: &str) -> Result<Vec<WorkspaceMembership>>;
    async fn set_workspace_quota(
        &self,
        workspace_id: &str,
        max_pages: Option<i64>,
        max_bytes: Option<i64>,
    ) -> Result<()>;
    /// Add or upsert a member. Fails with `NotFound` if the user doesn't exist.
    async fn add_member(&self, workspace_id: &str, user_id: &str, role: WorkspaceRole)
    -> Result<()>;
    async fn update_member_role(
        &self,
        workspace_id: &str,
        user_id: &str,
        role: WorkspaceRole,
    ) -> Result<()>;
    async fn remove_member(&self, workspace_id: &str, user_id: &str) -> Result<()>;
    async fn get_member_role(
        &self,
        workspace_id: &str,
        user_id: &str,
    ) -> Result<Option<WorkspaceRole>>;
    /// Members joined with usernames — for the members table UI.
    async fn list_members(&self, workspace_id: &str) -> Result<Vec<WorkspaceMemberView>>;
}
