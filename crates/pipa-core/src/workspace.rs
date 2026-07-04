//! Phase 4 workspaces. A `Workspace` owns pages under `owner_kind = "workspace"`.
//! Every user gets a `Personal` workspace on signup; `Team` workspaces are
//! shared. Membership carries a [`WorkspaceRole`] that gates what a member may
//! do. The Phase-1 single-owner (`owner_kind = "local"`) stays a superuser that
//! sits outside the workspace model.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceKind {
    Personal,
    Team,
}

impl WorkspaceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceKind::Personal => "personal",
            WorkspaceKind::Team => "team",
        }
    }
}

impl FromStr for WorkspaceKind {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "personal" => Ok(WorkspaceKind::Personal),
            "team" => Ok(WorkspaceKind::Team),
            other => Err(CoreError::InvalidInput(format!("unknown workspace kind: {other}"))),
        }
    }
}

/// A member's role in a workspace, ordered `Owner > Admin > Editor > Viewer`.
/// Capability checks below are the single source of truth for authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceRole {
    Owner,
    Admin,
    Editor,
    Viewer,
}

impl WorkspaceRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceRole::Owner => "owner",
            WorkspaceRole::Admin => "admin",
            WorkspaceRole::Editor => "editor",
            WorkspaceRole::Viewer => "viewer",
        }
    }

    /// Higher rank = more privilege. Use for `>=` comparisons.
    pub fn rank(&self) -> u8 {
        match self {
            WorkspaceRole::Owner => 3,
            WorkspaceRole::Admin => 2,
            WorkspaceRole::Editor => 1,
            WorkspaceRole::Viewer => 0,
        }
    }

    /// Read a page / stats. Every member can read.
    pub fn can_read(&self) -> bool {
        true
    }

    /// Deploy, delete, or change a page (editor and up).
    pub fn can_write(&self) -> bool {
        self.rank() >= WorkspaceRole::Editor.rank()
    }

    /// Invite/remove members, change roles, set quotas (admin and up).
    pub fn can_manage_members(&self) -> bool {
        self.rank() >= WorkspaceRole::Admin.rank()
    }

    /// Delete the whole workspace (owner only).
    pub fn can_delete_workspace(&self) -> bool {
        *self == WorkspaceRole::Owner
    }
}

impl FromStr for WorkspaceRole {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner" => Ok(WorkspaceRole::Owner),
            "admin" => Ok(WorkspaceRole::Admin),
            "editor" => Ok(WorkspaceRole::Editor),
            "viewer" => Ok(WorkspaceRole::Viewer),
            other => Err(CoreError::InvalidInput(format!("unknown workspace role: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub kind: WorkspaceKind,
    /// `None` = unlimited.
    pub max_pages: Option<i64>,
    /// `None` = unlimited.
    pub max_bytes: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkspace {
    pub name: String,
    pub kind: WorkspaceKind,
    pub max_pages: Option<i64>,
    pub max_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMember {
    pub workspace_id: String,
    pub user_id: String,
    pub role: WorkspaceRole,
    pub created_at: i64,
}

/// A workspace plus the querying user's role in it — returned by
/// `list_workspaces_for_user` so the CLI/UI can render "which workspaces am I in
/// and what can I do".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMembership {
    pub workspace: Workspace,
    pub role: WorkspaceRole,
}

/// A member row joined with the user's username — for the members table UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMemberView {
    pub user_id: String,
    pub username: String,
    pub role: WorkspaceRole,
    pub created_at: i64,
}
