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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Private,
    Public,
    Password,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Private => "private",
            Visibility::Public => "public",
            Visibility::Password => "password",
        }
    }
}

impl FromStr for Visibility {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "private" => Ok(Visibility::Private),
            "public" => Ok(Visibility::Public),
            "password" => Ok(Visibility::Password),
            other => Err(CoreError::InvalidInput(format!(
                "unknown visibility: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub uuid: String,
    pub name: Option<String>,
    pub mode: Mode,
    pub visibility: Visibility,
    pub password_hash: Option<String>,
    pub owner_kind: String,
    pub owner_id: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub comments_enabled: bool,
    pub comments_require_approval: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPage {
    pub uuid: String,
    pub name: Option<String>,
    pub mode: Mode,
    pub visibility: Visibility,
    pub password_hash: Option<String>,
    pub owner_kind: String,
    pub owner_id: String,
    pub size_bytes: u64,
    pub file_count: u64,
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
