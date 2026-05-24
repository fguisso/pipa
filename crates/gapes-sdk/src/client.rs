use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::Serialize;
use url::Url;

use crate::error::{ErrorBody, SdkError};
use crate::models::MintResponse;

/// Thin wrapper around `reqwest::Client` that knows the gapes base URL and
/// (optionally) a refresh token. The refresh token is only used by `mint`
/// — everything else takes an explicit `access` parameter so the CLI controls
/// rotation cadence.
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) base: Url,
    pub(crate) http: reqwest::Client,
    pub(crate) refresh: Option<String>,
}

impl Client {
    pub fn new(base_url: &str, refresh: Option<String>) -> Result<Self, SdkError> {
        let base = Url::parse(base_url)?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("gapes-sdk/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            base,
            http,
            refresh,
        })
    }

    /// Replace the stored refresh — used by callers after a `mint` to track
    /// the rotated token. Returns the previous one (if any) so it can be
    /// scrubbed by the caller.
    pub fn set_refresh(&mut self, refresh: Option<String>) -> Option<String> {
        std::mem::replace(&mut self.refresh, refresh)
    }

    pub fn refresh(&self) -> Option<&str> {
        self.refresh.as_deref()
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    pub(crate) fn url(&self, path: &str) -> Result<Url, SdkError> {
        // `Url::join` would drop everything after the last `/` in `base.path`,
        // so we splice manually to preserve any base path the operator picked.
        let base = self.base.as_str().trim_end_matches('/');
        let path = path.trim_start_matches('/');
        Ok(Url::parse(&format!("{base}/{path}"))?)
    }

    pub(crate) fn req(&self, method: Method, path: &str) -> Result<RequestBuilder, SdkError> {
        Ok(self.http.request(method, self.url(path)?))
    }

    /// Exchange the stored refresh for an access token at the given scope.
    /// Rotates the refresh — on success the client's stored refresh is
    /// updated and the new value is also returned for the caller to persist.
    pub async fn mint_access(&mut self, scope: &str, ttl_secs: u32) -> Result<MintResponse, SdkError> {
        let refresh = self
            .refresh
            .clone()
            .ok_or_else(|| SdkError::Decode("no refresh token configured".into()))?;

        #[derive(Serialize)]
        struct Body<'a> {
            refresh: &'a str,
            scope: &'a str,
            ttl_sec: u32,
        }
        let body = Body {
            refresh: &refresh,
            scope,
            ttl_sec: ttl_secs,
        };
        let resp = self
            .req(Method::POST, "/api/auth/mint")?
            .json(&body)
            .send()
            .await?;
        let mint: MintResponse = parse_json(resp).await?;
        self.refresh = Some(mint.refresh.clone());
        Ok(mint)
    }
}

/// Decode a JSON body and translate non-2xx into `SdkError::Api`.
pub(crate) async fn parse_json<T: serde::de::DeserializeOwned>(resp: Response) -> Result<T, SdkError> {
    let status = resp.status();
    if status.is_success() {
        // 204 has no body but no handler in this SDK currently expects T=().
        // Callers that want NO_CONTENT should use `parse_empty`.
        let text = resp.text().await?;
        return serde_json::from_str(&text)
            .map_err(|e| SdkError::Decode(format!("{e} (body: {})", truncate(&text))));
    }
    Err(api_error(status, resp).await)
}

/// Discard a successful empty body; map non-2xx to `SdkError::Api`.
pub(crate) async fn parse_empty(resp: Response) -> Result<(), SdkError> {
    let status = resp.status();
    if status.is_success() {
        let _ = resp.bytes().await?;
        return Ok(());
    }
    Err(api_error(status, resp).await)
}

async fn api_error(status: StatusCode, resp: Response) -> SdkError {
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return SdkError::Transport(e),
    };
    let body = serde_json::from_slice::<ErrorBody>(&bytes).ok();
    SdkError::Api {
        status: status.as_u16(),
        body,
    }
}

fn truncate(s: &str) -> String {
    const N: usize = 240;
    if s.len() <= N {
        s.into()
    } else {
        format!("{}…", &s[..N])
    }
}
