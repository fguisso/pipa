use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub id: i64,
    pub page_uuid: String,
    pub ts: i64,
    pub ip_hash: String,
    pub ua_hash: Option<String>,
    pub path: String,
    pub referrer: Option<String>,
    pub status: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewHit {
    pub page_uuid: String,
    pub ts: i64,
    pub ip_hash: String,
    pub ua_hash: Option<String>,
    pub path: String,
    pub referrer: Option<String>,
    pub status: i32,
}
