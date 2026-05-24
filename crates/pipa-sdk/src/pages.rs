//! `/api/pages*` calls — list, get, deploy, delete, visibility, stats.

use reqwest::Method;
use reqwest::multipart::{Form, Part};
use serde::Serialize;

use crate::client::{Client, parse_empty, parse_json};
use crate::error::SdkError;
use crate::models::{
    DeployParams, DeployResponse, ListPagesResponse, PageView, StatsResponse,
};

impl Client {
    pub async fn list_pages(&self, access: &str) -> Result<ListPagesResponse, SdkError> {
        let resp = self
            .req(Method::GET, "/api/pages")?
            .bearer_auth(access)
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn get_page(&self, access: &str, uuid: &str) -> Result<PageView, SdkError> {
        let resp = self
            .req(Method::GET, &format!("/api/pages/{uuid}"))?
            .bearer_auth(access)
            .send()
            .await?;
        parse_json(resp).await
    }

    /// Upload a pre-built zip archive. The CLI is responsible for zipping; we
    /// just frame the multipart and send it.
    pub async fn deploy_archive(
        &self,
        access: &str,
        archive_bytes: Vec<u8>,
        params: DeployParams,
    ) -> Result<DeployResponse, SdkError> {
        let mut form = Form::new().part(
            "archive",
            Part::bytes(archive_bytes)
                .file_name("archive.zip")
                .mime_str("application/zip")
                .map_err(SdkError::Transport)?,
        );
        if let Some(v) = params.uuid {
            form = form.text("uuid", v);
        }
        if let Some(v) = params.mode {
            form = form.text("mode", v);
        }
        if let Some(v) = params.name {
            form = form.text("name", v);
        }
        if let Some(v) = params.visibility {
            form = form.text("visibility", v);
        }
        if let Some(v) = params.password {
            form = form.text("password", v);
        }

        let resp = self
            .req(Method::POST, "/api/pages")?
            .bearer_auth(access)
            .multipart(form)
            .send()
            .await?;
        parse_json(resp).await
    }

    /// Step-up code is mandatory.
    pub async fn delete_page(
        &self,
        access: &str,
        uuid: &str,
        stepup_code: &str,
    ) -> Result<(), SdkError> {
        let resp = self
            .req(Method::DELETE, &format!("/api/pages/{uuid}"))?
            .bearer_auth(access)
            .header("X-Stepup-Code", stepup_code)
            .send()
            .await?;
        parse_empty(resp).await
    }

    /// `visibility` is one of `private | public | password`. `password` is
    /// only consulted (and only required) when `visibility == "password"`.
    /// `stepup_code` is required only when moving to `public`.
    pub async fn set_visibility(
        &self,
        access: &str,
        uuid: &str,
        visibility: &str,
        password: Option<&str>,
        stepup_code: Option<&str>,
    ) -> Result<PageView, SdkError> {
        #[derive(Serialize)]
        struct Body<'a> {
            visibility: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            password: Option<&'a str>,
        }
        let mut req = self
            .req(Method::POST, &format!("/api/pages/{uuid}/visibility"))?
            .bearer_auth(access);
        if let Some(c) = stepup_code {
            req = req.header("X-Stepup-Code", c);
        }
        let resp = req
            .json(&Body {
                visibility,
                password,
            })
            .send()
            .await?;
        parse_json(resp).await
    }

    /// `range` is one of `24h | 7d | 30d | all`.
    pub async fn stats(
        &self,
        access: &str,
        uuid: &str,
        range: &str,
    ) -> Result<StatsResponse, SdkError> {
        let resp = self
            .req(Method::GET, &format!("/api/pages/{uuid}/stats"))?
            .bearer_auth(access)
            .query(&[("range", range)])
            .send()
            .await?;
        parse_json(resp).await
    }
}
