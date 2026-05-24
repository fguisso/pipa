use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Interactive,
    Automation,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::Interactive => "interactive",
            Scope::Automation => "automation",
        }
    }
}

impl FromStr for Scope {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "interactive" => Ok(Scope::Interactive),
            "automation" => Ok(Scope::Automation),
            other => Err(CoreError::InvalidInput(format!("unknown scope: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub label: String,
    pub scope: Scope,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshToken {
    pub id: String,
    pub device_id: String,
    pub token_hash: String,
    pub scope: Scope,
    pub created_at: i64,
    pub expires_at: i64,
    pub rotated_to: Option<String>,
    pub revoked_at: Option<i64>,
}

/// Claims encoded in an access token. Access tokens themselves live only in
/// the CLI process memory and are sent as `Authorization: Bearer …`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: String,
    pub scope: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepUpToken {
    pub code: String,
    pub device_id: String,
    pub operation: String,
    pub target: Option<String>,
    pub requesting_ip_hash: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
    pub confirmed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupCode {
    pub code: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
}

/// The server's admin user. Phase 1 is single-owner — at most one row
/// exists. Created at first `/setup` via the username + password wizard.
/// Holds a reference to a synthetic device row used to mint access tokens
/// for admin-UI handlers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Admin {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub synthetic_device_id: String,
    pub created_at: i64,
}

/// A browser session that belongs to the admin. Identifies the holder of the
/// signed `pipa_owner` cookie. Created at admin signup and on every
/// successful `/admin/login`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerSession {
    pub id: String,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePairing {
    pub code: String,
    pub secret_hash: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub approved_device_id: Option<String>,
    pub approved_at: Option<i64>,
    pub refresh_token_id: Option<String>,
}
