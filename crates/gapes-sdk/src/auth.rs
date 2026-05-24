//! Auth endpoints: device flow, step-up, logout. `mint` lives on `Client`
//! because it mutates the stored refresh.

use reqwest::Method;
use serde::Serialize;

use crate::client::{Client, parse_empty, parse_json};
use crate::error::SdkError;
use crate::models::{
    DeviceInitResponse, DevicePoll, StepUpInitResponse, StepUpStatusResponse,
};

impl Client {
    /// Begin a device pairing. First-device callers pass `setup_code`;
    /// subsequent devices pass an existing `manage:devices` access token via
    /// `bearer` (the server requires one).
    pub async fn device_init(
        &self,
        scope: &str,
        client_label_hint: Option<&str>,
        setup_code: Option<&str>,
        bearer: Option<&str>,
    ) -> Result<DeviceInitResponse, SdkError> {
        #[derive(Serialize)]
        struct Body<'a> {
            scope: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            client_label_hint: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            setup_code: Option<&'a str>,
        }
        let body = Body {
            scope,
            client_label_hint,
            setup_code,
        };
        let mut req = self.req(Method::POST, "/api/auth/device-init")?;
        if let Some(b) = bearer {
            req = req.bearer_auth(b);
        }
        let resp = req.json(&body).send().await?;
        parse_json(resp).await
    }

    pub async fn device_poll(
        &self,
        device_code: &str,
        device_secret: &str,
    ) -> Result<DevicePoll, SdkError> {
        #[derive(Serialize)]
        struct Body<'a> {
            device_code: &'a str,
            device_secret: &'a str,
        }
        let resp = self
            .req(Method::POST, "/api/auth/device-poll")?
            .json(&Body {
                device_code,
                device_secret,
            })
            .send()
            .await?;
        parse_json(resp).await
    }

    /// Revoke the current device. The bearer is mandatory because it's how
    /// the server knows which device to invalidate.
    pub async fn logout(&self, access: &str) -> Result<(), SdkError> {
        let resp = self
            .req(Method::POST, "/api/auth/logout")?
            .bearer_auth(access)
            .send()
            .await?;
        parse_empty(resp).await
    }

    pub async fn stepup_init(
        &self,
        access: &str,
        operation: &str,
        target: Option<&str>,
    ) -> Result<StepUpInitResponse, SdkError> {
        #[derive(Serialize)]
        struct Body<'a> {
            operation: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            target: Option<&'a str>,
        }
        let resp = self
            .req(Method::POST, "/api/auth/stepup-init")?
            .bearer_auth(access)
            .json(&Body { operation, target })
            .send()
            .await?;
        parse_json(resp).await
    }

    pub async fn stepup_status(&self, code: &str) -> Result<StepUpStatusResponse, SdkError> {
        #[derive(Serialize)]
        struct Body<'a> {
            code: &'a str,
        }
        let resp = self
            .req(Method::POST, "/api/auth/stepup-status")?
            .json(&Body { code })
            .send()
            .await?;
        parse_json(resp).await
    }
}
