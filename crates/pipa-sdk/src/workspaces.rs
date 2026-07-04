//! `/api/workspaces*` calls + page transfer (Phase 4).

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::client::{Client, parse_json};
use crate::error::SdkError;
use crate::models::PageView;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub max_pages: Option<i64>,
    pub max_bytes: Option<i64>,
    pub created_at: i64,
}

/// A workspace plus the caller's role in it (from `GET /api/workspaces`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub workspace: WorkspaceInfo,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberInfo {
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDetail {
    pub workspace: WorkspaceInfo,
    pub my_role: String,
    pub members: Vec<MemberInfo>,
}

impl Client {
    pub async fn list_workspaces(&self, access: &str) -> Result<Vec<Membership>, SdkError> {
        let resp = self
            .req(Method::GET, "/api/workspaces")?
            .bearer_auth(access)
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn create_workspace(
        &self,
        access: &str,
        name: &str,
    ) -> Result<WorkspaceInfo, SdkError> {
        let resp = self
            .req(Method::POST, "/api/workspaces")?
            .bearer_auth(access)
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn get_workspace(
        &self,
        access: &str,
        id: &str,
    ) -> Result<WorkspaceDetail, SdkError> {
        let resp = self
            .req(Method::GET, &format!("/api/workspaces/{id}"))?
            .bearer_auth(access)
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn add_member(
        &self,
        access: &str,
        id: &str,
        username: &str,
        role: &str,
    ) -> Result<Vec<MemberInfo>, SdkError> {
        let resp = self
            .req(Method::POST, &format!("/api/workspaces/{id}/members"))?
            .bearer_auth(access)
            .json(&serde_json::json!({ "username": username, "role": role }))
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn set_member_role(
        &self,
        access: &str,
        id: &str,
        user_id: &str,
        role: &str,
    ) -> Result<Vec<MemberInfo>, SdkError> {
        let resp = self
            .req(
                Method::POST,
                &format!("/api/workspaces/{id}/members/{user_id}/role"),
            )?
            .bearer_auth(access)
            .json(&serde_json::json!({ "role": role }))
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn remove_member(
        &self,
        access: &str,
        id: &str,
        user_id: &str,
    ) -> Result<Vec<MemberInfo>, SdkError> {
        let resp = self
            .req(
                Method::DELETE,
                &format!("/api/workspaces/{id}/members/{user_id}"),
            )?
            .bearer_auth(access)
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn set_quota(
        &self,
        access: &str,
        id: &str,
        max_pages: Option<i64>,
        max_bytes: Option<i64>,
    ) -> Result<WorkspaceInfo, SdkError> {
        let resp = self
            .req(Method::POST, &format!("/api/workspaces/{id}/quota"))?
            .bearer_auth(access)
            .json(&serde_json::json!({ "max_pages": max_pages, "max_bytes": max_bytes }))
            .send()
            .await?;
        parse_json(resp).await
    }

    /// Move a page to another workspace.
    pub async fn transfer_page(
        &self,
        access: &str,
        uuid: &str,
        workspace: &str,
    ) -> Result<PageView, SdkError> {
        let resp = self
            .req(Method::POST, &format!("/api/pages/{uuid}/transfer"))?
            .bearer_auth(access)
            .json(&serde_json::json!({ "workspace": workspace }))
            .send()
            .await?;
        parse_json(resp).await
    }
}
