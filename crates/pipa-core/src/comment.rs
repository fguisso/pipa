use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentStatus {
    Visible,
    Pending,
    Hidden,
}

impl CommentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommentStatus::Visible => "visible",
            CommentStatus::Pending => "pending",
            CommentStatus::Hidden => "hidden",
        }
    }
}

impl FromStr for CommentStatus {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "visible" => Ok(CommentStatus::Visible),
            "pending" => Ok(CommentStatus::Pending),
            "hidden" => Ok(CommentStatus::Hidden),
            other => Err(CoreError::InvalidInput(format!(
                "unknown comment status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub page_uuid: String,
    pub author: String,
    pub body_md: String,
    pub body_html: String,
    pub contact: Option<String>,
    pub ts: i64,
    pub ip_hash: String,
    pub status: CommentStatus,
    pub user_agent: Option<String>,
    pub anchor_selector: String,
    pub anchor_text: String,
    pub anchor_offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewComment {
    pub id: String,
    pub page_uuid: String,
    pub author: String,
    pub body_md: String,
    pub body_html: String,
    pub contact: Option<String>,
    pub ts: i64,
    pub ip_hash: String,
    pub status: CommentStatus,
    pub user_agent: Option<String>,
    pub anchor_selector: String,
    pub anchor_text: String,
    pub anchor_offset: i64,
}
