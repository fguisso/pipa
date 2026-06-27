use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Static,
    Spa,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Static => "static",
            Mode::Spa => "spa",
        }
    }
}

impl FromStr for Mode {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "static" => Ok(Mode::Static),
            "spa" => Ok(Mode::Spa),
            other => Err(CoreError::InvalidInput(format!("unknown mode: {other}"))),
        }
    }
}

/// How a visitor authenticates to open a page — the "who can enter" axis.
/// Orthogonal to [`Zone`] (the "where is it reachable" axis). `Password`
/// (the secure default) gates the page behind a shared secret; `Noauth`
/// serves it to anyone who can reach it. Future methods (SSO, social, magic
/// link) slot in here behind their own Cargo features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    Password,
    Noauth,
}

impl Access {
    pub fn as_str(&self) -> &'static str {
        match self {
            Access::Password => "password",
            Access::Noauth => "noauth",
        }
    }
}

impl FromStr for Access {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "password" => Ok(Access::Password),
            "noauth" => Ok(Access::Noauth),
            other => Err(CoreError::InvalidInput(format!("unknown access: {other}"))),
        }
    }
}

/// Where a page is reachable — the "which network" axis, enforced as an exact
/// match: a `Private` page serves only on the internal (LAN) channel, a
/// `Public` page only on the external (internet) channel. Enforcement lives behind
/// the `zone` Cargo feature in `pipa-server`; the field is always stored so
/// the schema stays build-compatible. Designed to grow past two values later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Zone {
    Public,
    Private,
}

impl Zone {
    pub fn as_str(&self) -> &'static str {
        match self {
            Zone::Public => "public",
            Zone::Private => "private",
        }
    }
}

impl FromStr for Zone {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "public" => Ok(Zone::Public),
            "private" => Ok(Zone::Private),
            other => Err(CoreError::InvalidInput(format!("unknown zone: {other}"))),
        }
    }
}

/// Per-page CSP setting. `Strict` (default) emits the platform's hardened
/// `Content-Security-Policy` header on every response. `Off` suppresses the
/// header entirely so the page can declare its own policy via a
/// `<meta http-equiv="Content-Security-Policy">` tag — necessary for sites
/// that legitimately load assets from CDNs (React, Babel, icon fonts, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Csp {
    Strict,
    Off,
}

impl Csp {
    pub fn as_str(&self) -> &'static str {
        match self {
            Csp::Strict => "strict",
            Csp::Off => "off",
        }
    }
}

impl FromStr for Csp {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "strict" => Ok(Csp::Strict),
            "off" => Ok(Csp::Off),
            other => Err(CoreError::InvalidInput(format!("unknown csp: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub uuid: String,
    pub name: Option<String>,
    pub mode: Mode,
    pub access: Access,
    pub zone: Zone,
    pub password_hash: Option<String>,
    pub owner_kind: String,
    pub owner_id: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub comments_enabled: bool,
    pub comments_require_approval: bool,
    pub csp: Csp,
    /// Soft-unpublished: serving layer 404s, files preserved on disk.
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPage {
    pub uuid: String,
    pub name: Option<String>,
    pub mode: Mode,
    pub access: Access,
    pub zone: Zone,
    pub password_hash: Option<String>,
    pub owner_kind: String,
    pub owner_id: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub csp: Csp,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageStats {
    pub views: u64,
    pub uniques: u64,
    pub top_paths: Vec<(String, u64)>,
    pub top_referrers: Vec<(String, u64)>,
}
