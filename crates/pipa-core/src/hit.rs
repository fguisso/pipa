use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Whether a recorded hit was a page view — an HTML document a human navigated
/// to — or a sub-resource (`Asset`) fetch (CSS, JS, fonts, images) pulled in as
/// a side effect of that navigation. Analytics headline metrics count `Page`
/// hits only: one browser navigation is one view, not one-per-asset. Classified
/// at record time by the serving layer, which knows the response's content type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HitKind {
    Page,
    Asset,
}

impl HitKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HitKind::Page => "page",
            HitKind::Asset => "asset",
        }
    }
}

impl FromStr for HitKind {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "page" => Ok(HitKind::Page),
            "asset" => Ok(HitKind::Asset),
            other => Err(CoreError::InvalidInput(format!("unknown hit kind: {other}"))),
        }
    }
}

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
    pub kind: HitKind,
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
    pub kind: HitKind,
}
