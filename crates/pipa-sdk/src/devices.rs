use reqwest::Method;

use crate::client::{Client, parse_empty, parse_json};
use crate::error::SdkError;
use crate::models::ListDevicesResponse;

impl Client {
    pub async fn list_devices(&self, access: &str) -> Result<ListDevicesResponse, SdkError> {
        let resp = self
            .req(Method::GET, "/api/devices")?
            .bearer_auth(access)
            .send()
            .await?;
        parse_json(resp).await
    }

    /// Self-revocation does not need a step-up code; revoking any other
    /// device does. Pass `None` when revoking self.
    pub async fn revoke_device(
        &self,
        access: &str,
        id: &str,
        stepup_code: Option<&str>,
    ) -> Result<(), SdkError> {
        let mut req = self
            .req(Method::DELETE, &format!("/api/devices/{id}"))?
            .bearer_auth(access);
        if let Some(c) = stepup_code {
            req = req.header("X-Stepup-Code", c);
        }
        let resp = req.send().await?;
        parse_empty(resp).await
    }
}
