//! Serde mirrors of the server's wire types. Kept dependency-free of
//! `pipa-core` so the SDK can be embedded by callers that don't want the
//! whole core graph.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageView {
    pub uuid: String,
    pub name: Option<String>,
    pub mode: String,
    pub visibility: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub size_bytes: u64,
    pub file_count: u64,
    #[serde(default)]
    pub comments_enabled: bool,
    #[serde(default)]
    pub comments_require_approval: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPagesResponse {
    pub pages: Vec<PageView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResponse {
    pub uuid: String,
    pub url: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub mode: String,
    pub visibility: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeployParams {
    pub uuid: Option<String>,
    pub mode: Option<String>,
    pub name: Option<String>,
    pub visibility: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageStats {
    pub views: u64,
    pub uniques: u64,
    pub top_paths: Vec<(String, u64)>,
    pub top_referrers: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub range: String,
    pub since_ts: i64,
    pub stats: PageStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintResponse {
    pub access: String,
    pub refresh: String,
    pub expires: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInitResponse {
    pub device_code: String,
    pub device_secret: String,
    pub verify_url: String,
    pub expires_in: i64,
}

/// Mirrors `DevicePollResponse` from the server. `status` is the discriminant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DevicePoll {
    Pending,
    Approved {
        refresh_token: String,
        device_id: String,
        device_label: String,
        scope: String,
        server: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepUpInitResponse {
    pub code: String,
    pub verify_url: String,
    pub expires_in: i64,
    pub operation: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepUpStatusResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub label: String,
    pub scope: String,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDevicesResponse {
    pub devices: Vec<Device>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub page_uuid: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub author: String,
    #[serde(default)]
    pub body_md: String,
    pub body_html: String,
    #[serde(default)]
    pub contact: Option<String>,
    pub ts: i64,
    #[serde(default)]
    pub ip_hash: Option<String>,
    pub status: String,
    #[serde(default)]
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommentsConfig {
    pub enabled: bool,
    pub require_approval: bool,
}
