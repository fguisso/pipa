//! Comments endpoints. The server currently stubs these (M5 will implement),
//! so calls will return 501 until then. The SDK shape mirrors the spec in
//! `phase-1-comments.md` so the CLI can be wired up now and Just Work once
//! the server lands.

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::client::{Client, parse_empty, parse_json};
use crate::error::SdkError;
use crate::models::{Comment, CommentsConfig};

#[derive(Debug, Clone, Deserialize)]
pub struct CommentsList {
    pub comments: Vec<Comment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnchorData<'a> {
    pub selector: &'a str,
    pub text: &'a str,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewCommentRequest<'a> {
    pub author: &'a str,
    pub body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<&'a str>,
    pub anchor: AnchorData<'a>,
}

impl Client {
    pub async fn comments_list(
        &self,
        page_uuid: &str,
    ) -> Result<CommentsList, SdkError> {
        let resp = self
            .req(
                Method::GET,
                &format!("/api/pages/{page_uuid}/comments"),
            )?
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn comments_post<'a>(
        &self,
        page_uuid: &str,
        req: &NewCommentRequest<'a>,
    ) -> Result<Comment, SdkError> {
        let resp = self
            .req(Method::POST, &format!("/api/pages/{page_uuid}/comments"))?
            .json(req)
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn comments_moderation_list(
        &self,
        access: &str,
        page_uuid: &str,
    ) -> Result<CommentsList, SdkError> {
        let resp = self
            .req(
                Method::GET,
                &format!("/api/pages/{page_uuid}/comments/moderation"),
            )?
            .bearer_auth(access)
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn comments_set_status(
        &self,
        access: &str,
        id: &str,
        status: &str,
    ) -> Result<(), SdkError> {
        #[derive(Serialize)]
        struct Body<'a> {
            status: &'a str,
        }
        let resp = self
            .req(Method::PATCH, &format!("/api/comments/{id}"))?
            .bearer_auth(access)
            .json(&Body { status })
            .send()
            .await?;
        parse_empty(resp).await
    }

    pub async fn comments_delete(&self, access: &str, id: &str) -> Result<(), SdkError> {
        let resp = self
            .req(Method::DELETE, &format!("/api/comments/{id}"))?
            .bearer_auth(access)
            .send()
            .await?;
        parse_empty(resp).await
    }

    /// Set the page-level comments config (M5 endpoint). Until the server
    /// ships the route, this returns 501 — the CLI catches and explains.
    pub async fn comments_set_config(
        &self,
        access: &str,
        page_uuid: &str,
        config: &CommentsConfig,
    ) -> Result<(), SdkError> {
        let resp = self
            .req(
                Method::POST,
                &format!("/api/pages/{page_uuid}/comments/config"),
            )?
            .bearer_auth(access)
            .json(config)
            .send()
            .await?;
        parse_empty(resp).await
    }
}
