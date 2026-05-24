//! All Phase-1 surfaces now have real handlers — `/admin/*` is owned by the
//! `admin` module (M7). This module is intentionally empty; it remains so
//! future phases can re-introduce stubs without re-wiring `mod.rs`.

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
}
