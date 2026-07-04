//! Phase 3 multi-user model. A `User` is a self-service account (username +
//! password) that owns pages under `owner_kind = "user"`, alongside the
//! Phase-1 single-owner (`owner_kind = "local"`) rows which keep working.
//!
//! `UserSession` is the browser session behind the signed `pipa_user` cookie
//! (mirrors the admin's `OwnerSession`). `OAuthIdentity` is scaffold only —
//! the table + links exist so Phase 5 can wire real GitHub/Google flows without
//! another migration; no live OAuth happens this phase.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub created_at: i64,
    /// Soft-disable: a non-null timestamp blocks login without deleting data.
    pub disabled_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewUser {
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub id: String,
    pub user_id: String,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    pub revoked_at: Option<i64>,
}

/// OAuth provider identifier. Scaffold only in Phase 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProvider {
    Github,
    Google,
}

impl OAuthProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            OAuthProvider::Github => "github",
            OAuthProvider::Google => "google",
        }
    }
}

impl FromStr for OAuthProvider {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "github" => Ok(OAuthProvider::Github),
            "google" => Ok(OAuthProvider::Google),
            other => Err(CoreError::InvalidInput(format!(
                "unknown oauth provider: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthIdentity {
    pub id: String,
    pub user_id: String,
    pub provider: OAuthProvider,
    pub subject: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewOAuthIdentity {
    pub user_id: String,
    pub provider: OAuthProvider,
    pub subject: String,
}
