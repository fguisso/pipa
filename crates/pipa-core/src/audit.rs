use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    OwnerClaim,
    AuthLogin,
    AuthRefresh,
    AuthRevoke,
    PageCreate,
    PageUpdate,
    PageDelete,
    PageVisibilityChange,
    DeviceRevoke,
    CommentCreate,
    CommentApprove,
    CommentHide,
    CommentDelete,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::OwnerClaim => "owner.claim",
            AuditAction::AuthLogin => "auth.login",
            AuditAction::AuthRefresh => "auth.refresh",
            AuditAction::AuthRevoke => "auth.revoke",
            AuditAction::PageCreate => "page.create",
            AuditAction::PageUpdate => "page.update",
            AuditAction::PageDelete => "page.delete",
            AuditAction::PageVisibilityChange => "page.visibility_change",
            AuditAction::DeviceRevoke => "device.revoke",
            AuditAction::CommentCreate => "comment.create",
            AuditAction::CommentApprove => "comment.approve",
            AuditAction::CommentHide => "comment.hide",
            AuditAction::CommentDelete => "comment.delete",
        }
    }
}

impl FromStr for AuditAction {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner.claim" => Ok(AuditAction::OwnerClaim),
            "auth.login" => Ok(AuditAction::AuthLogin),
            "auth.refresh" => Ok(AuditAction::AuthRefresh),
            "auth.revoke" => Ok(AuditAction::AuthRevoke),
            "page.create" => Ok(AuditAction::PageCreate),
            "page.update" => Ok(AuditAction::PageUpdate),
            "page.delete" => Ok(AuditAction::PageDelete),
            "page.visibility_change" => Ok(AuditAction::PageVisibilityChange),
            "device.revoke" => Ok(AuditAction::DeviceRevoke),
            "comment.create" => Ok(AuditAction::CommentCreate),
            "comment.approve" => Ok(AuditAction::CommentApprove),
            "comment.hide" => Ok(AuditAction::CommentHide),
            "comment.delete" => Ok(AuditAction::CommentDelete),
            other => Err(CoreError::InvalidInput(format!(
                "unknown audit action: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Option<i64>,
    pub ts: i64,
    pub actor: String,
    pub ip_hash: Option<String>,
    pub scope: Option<String>,
    pub action: AuditAction,
    pub target: Option<String>,
    pub success: bool,
    pub details: Option<String>,
}

impl AuditEvent {
    pub fn success(ts: i64, actor: impl Into<String>, action: AuditAction) -> Self {
        Self {
            id: None,
            ts,
            actor: actor.into(),
            ip_hash: None,
            scope: None,
            action,
            target: None,
            success: true,
            details: None,
        }
    }

    pub fn failure(ts: i64, actor: impl Into<String>, action: AuditAction) -> Self {
        Self {
            id: None,
            ts,
            actor: actor.into(),
            ip_hash: None,
            scope: None,
            action,
            target: None,
            success: false,
            details: None,
        }
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    pub fn with_ip_hash(mut self, ip_hash: impl Into<String>) -> Self {
        self.ip_hash = Some(ip_hash.into());
        self
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}
