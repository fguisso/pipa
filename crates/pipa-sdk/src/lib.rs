//! pipa HTTP API client. Wraps the JSON / multipart endpoints exposed by
//! `pipa-server` so a third-party tool can script deploys, list pages, manage
//! comments, etc. The CLI (`pipa-cli`) is the first consumer.
//!
//! Layout: one module per endpoint family (`auth`, `pages`, `comments`,
//! `devices`) plus shared `client`, `models`, `error`.
//!
//! The client is intentionally low-level: callers pass in an `access` token
//! per call. Token rotation cadence is the caller's choice — `Client::mint`
//! returns the new refresh + access pair, and the CLI typically mints fresh
//! per command.

pub mod auth;
pub mod client;
pub mod comments;
pub mod devices;
pub mod error;
pub mod models;
pub mod meta;
pub mod pages;
pub mod workspaces;

pub use client::Client;
pub use error::{ErrorBody, SdkError};
pub use models::*;
pub use workspaces::{Membership, MemberInfo, WorkspaceDetail, WorkspaceInfo};
