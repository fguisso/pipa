use reqwest::Method;

use crate::client::{Client, parse_json};
use crate::error::SdkError;
use crate::models::MetaResponse;

impl Client {
    /// Authenticated capability discovery: which optional features this server
    /// actually enforces (e.g. `zone`). Lets a client gate feature-dependent
    /// flags before sending a request the server would silently ignore.
    pub async fn meta(&self, access: &str) -> Result<MetaResponse, SdkError> {
        let resp = self
            .req(Method::GET, "/api/meta")?
            .bearer_auth(access)
            .send()
            .await?;
        parse_json(resp).await
    }
}
