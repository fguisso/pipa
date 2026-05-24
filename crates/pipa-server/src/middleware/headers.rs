use axum::http::header::{HeaderName, HeaderValue};
use axum::http::{Request, Response};
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// CSP applied only to `/p/<uuid>/*` responses, per SECURITY.md §2. Kept off
/// the admin / API surface so future admin UI tweaks don't have to fight it.
pub const PAGE_CSP: &str = "default-src 'self'; base-uri 'self'; form-action 'self'";

static SERVER_HEADER: HeaderName = HeaderName::from_static("server");
static X_POWERED_BY: HeaderName = HeaderName::from_static("x-powered-by");

/// Global header hardening: nosniff, referrer policy, minimal Permissions-
/// Policy, and aggressive removal of `Server` / `X-Powered-By` headers that
/// downstream layers (or hyper itself) might have set.
#[derive(Clone, Copy, Default)]
pub struct SecurityHeadersLayer;

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeaders<S>;
    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeaders { inner }
    }
}

#[derive(Clone)]
pub struct SecurityHeaders<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for SecurityHeaders<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let mut resp = inner.call(req).await?;
            let headers = resp.headers_mut();
            headers.insert(
                "x-content-type-options",
                HeaderValue::from_static("nosniff"),
            );
            headers.insert(
                "referrer-policy",
                HeaderValue::from_static("strict-origin-when-cross-origin"),
            );
            headers.insert(
                "permissions-policy",
                HeaderValue::from_static(
                    "accelerometer=(), camera=(), geolocation=(), microphone=(), payment=()",
                ),
            );
            headers.remove(&SERVER_HEADER);
            headers.remove(&X_POWERED_BY);
            Ok(resp)
        })
    }
}

/// Per-route layer that adds the strict page CSP. Applied only to the
/// `/p/<uuid>/*` router so it doesn't leak into admin / auth pages.
#[derive(Clone, Copy, Default)]
pub struct PageCspLayer;

impl<S> Layer<S> for PageCspLayer {
    type Service = PageCsp<S>;
    fn layer(&self, inner: S) -> Self::Service {
        PageCsp { inner }
    }
}

#[derive(Clone)]
pub struct PageCsp<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for PageCsp<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let mut resp = inner.call(req).await?;
            resp.headers_mut().insert(
                "content-security-policy",
                HeaderValue::from_static(PAGE_CSP),
            );
            Ok(resp)
        })
    }
}
