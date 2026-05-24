//! Server-side auth primitives: access-token mint/verify, the Bearer
//! extractor, and scope helpers. Designed to be opaque to the client: tokens
//! are `<base64url(payload)>.<base64url(hmac)>` strings the CLI just echoes
//! back in `Authorization: Bearer`.
//!
//! Why not JWT? We control both ends, do not need third-party algorithms, and
//! want the smallest plausible attack surface. The HMAC is keyed by the
//! existing `HmacKey` and domain-separated with `"access-token/v1/"`.

pub mod extractor;
pub mod scope;
pub mod tokens;

pub use extractor::AuthClaims;
#[allow(unused_imports)]
pub use scope::{ScopeRef, check_scope, parse_scope};
#[allow(unused_imports)]
pub use tokens::{
    issue_step_up_cookie, mint_access_token, verify_access_token, verify_step_up,
};
