use std::sync::Arc;

use pipa_adapters::{Config, HmacKey};
use pipa_core::{AuthStore, Repository, Storage};

use crate::ip_hash::SaltStore;
use crate::middleware::rate_limit::CommentLimiter;

pub type DynRepository = Arc<dyn Repository>;
pub type DynAuthStore = Arc<dyn AuthStore>;
pub type DynStorage = Arc<dyn Storage>;

/// Shared application state. `AppState` is cheap to clone; the heavy fields
/// are all `Arc`-wrapped so axum's `with_state` clone-on-handle pattern works
/// without copying real data.
#[derive(Clone)]
pub struct AppState {
    pub repo: DynRepository,
    pub auth: DynAuthStore,
    pub storage: DynStorage,
    pub hmac_key: HmacKey,
    pub config: Arc<Config>,
    pub salts: Arc<SaltStore>,
    pub comment_limiter: Arc<CommentLimiter>,
}

impl AppState {
    pub fn new(
        repo: DynRepository,
        auth: DynAuthStore,
        storage: DynStorage,
        hmac_key: HmacKey,
        config: Config,
    ) -> Self {
        Self {
            repo,
            auth,
            storage,
            hmac_key,
            config: Arc::new(config),
            salts: Arc::new(SaltStore::new()),
            comment_limiter: Arc::new(CommentLimiter::new()),
        }
    }
}
