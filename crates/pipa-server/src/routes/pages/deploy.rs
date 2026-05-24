//! `POST /api/pages` — deploy a zip archive to a new or existing page.
//!
//! The body is multipart/form-data with one mandatory `archive` part and a
//! handful of optional text parts (`uuid`, `mode`, `name`, `visibility`,
//! `password`). Flow:
//!
//! 1. Walk the multipart and collect fields. The archive part is bounded by
//!    `config.hosting.max_upload_bytes`; the route-level `RequestBodyLimit`
//!    catches grossly oversized bodies before they reach us, this is the
//!    fine-grained per-field check.
//! 2. Validate scope. `uuid` absent → `deploy:new`; present → `deploy:<uuid>`
//!    (or wildcard `deploy:*`).
//! 3. Run the zip through `extract_entries` in `spawn_blocking` — that
//!    function only decompresses + sanitizes, it doesn't touch the FS.
//! 4. `storage.begin_staging()`, `put_staged` each entry, `promote(handle,
//!    uuid)`.
//! 5. Insert or update the `pages` row with the new metadata, hash any
//!    password, audit, respond.

use std::io::{Cursor, Read};

use axum::Json;
use axum::extract::{Multipart, State};
use bytes::Bytes;
use pipa_adapters::hash_password;
use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::ids::UlidGen;
use pipa_core::page::{Mode, NewPage, Visibility};
use pipa_core::ports::PromotedInfo;
use pipa_core::{IdGen, Page};
use serde::Serialize;

use crate::auth::{AuthClaims, check_scope};
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::util::{OWNER_ID_LOCAL, OWNER_KIND_LOCAL, unix_now};

/// Decompressed bytes are allowed to exceed compressed by 2x — generous
/// enough for legitimate HTML/JS bundles, tight enough to stop trivial bombs.
const ZIP_BOMB_MULTIPLIER: u64 = 2;

/// Cap on the number of files extracted per archive.
const MAX_FILES_PER_ARCHIVE: usize = 5_000;

#[derive(Debug, Default)]
struct Form {
    archive: Option<Bytes>,
    uuid: Option<String>,
    mode: Option<String>,
    name: Option<String>,
    visibility: Option<String>,
    password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeployResponse {
    pub uuid: String,
    pub url: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub mode: String,
    pub visibility: String,
}

pub async fn deploy(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    multipart: Multipart,
) -> Result<Json<DeployResponse>, ServerError> {
    let form = read_form(multipart, state.config.hosting.max_upload_bytes).await?;

    let archive = form
        .archive
        .ok_or_else(|| ApiError::bad_request("missing_archive", "archive field is required"))?;

    // Authorization. Updating an existing UUID requires deploy:<uuid> (or
    // deploy:*); creating requires deploy:new.
    let updating = form.uuid.is_some();
    if updating {
        let uuid = form.uuid.as_deref().unwrap();
        if !check_scope(&claims, "deploy", Some(uuid)) {
            return Err(ApiError::forbidden(
                "insufficient_scope",
                format!("deploy:{uuid} or deploy:* scope required"),
            )
            .into());
        }
    } else if !check_scope(&claims, "deploy", Some("new")) {
        return Err(ApiError::forbidden(
            "insufficient_scope",
            "deploy:new scope required to create a page",
        )
        .into());
    }

    // Resolve / mint the page UUID before extraction so a successful
    // extraction can be promoted immediately.
    let page_uuid = if let Some(u) = form.uuid.clone() {
        u
    } else {
        UlidGen.new_ulid().to_string()
    };

    // If updating, the row must already exist so we don't silently materialize
    // pages the caller didn't actually create.
    let existing: Option<Page> = if updating {
        let p = state
            .repo
            .find_page(&page_uuid)
            .await?
            .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;
        Some(p)
    } else {
        None
    };

    // Mode / visibility resolution: explicit form field wins; otherwise keep
    // the existing value (update) or fall back to config default / private
    // (create). Never silently flip a public page back to private just
    // because the caller didn't re-state the visibility.
    let mode: Mode = match form.mode.as_deref() {
        Some(s) => s.parse().map_err(|_| {
            ApiError::bad_request("invalid_mode", "mode must be static|spa")
        })?,
        None => existing
            .as_ref()
            .map(|p| p.mode)
            .unwrap_or_else(|| state.config.hosting.default_mode.parse().unwrap_or(Mode::Spa)),
    };

    let visibility: Visibility = match form.visibility.as_deref() {
        Some(s) => s.parse().map_err(|_| {
            ApiError::bad_request(
                "invalid_visibility",
                "visibility must be private|public|password",
            )
        })?,
        None => existing
            .as_ref()
            .map(|p| p.visibility)
            .unwrap_or(Visibility::Private),
    };

    // For visibility=password the password is mandatory on create or when
    // switching into password mode; if updating a page that is already
    // password-protected and the caller didn't re-supply the password, keep
    // the existing hash. For non-password visibilities the field is ignored.
    let password_hash: Option<String> = if visibility == Visibility::Password {
        match form.password.clone().filter(|s| !s.is_empty()) {
            Some(plaintext) => {
                let h = tokio::task::spawn_blocking(move || hash_password(&plaintext))
                    .await
                    .map_err(|e| anyhow::anyhow!("argon2 join: {e}"))?
                    .map_err(ServerError::Internal)?;
                Some(h)
            }
            None => existing
                .as_ref()
                .and_then(|p| p.password_hash.clone())
                .ok_or_else(|| {
                    ApiError::bad_request(
                        "missing_password",
                        "password field is required when visibility=password",
                    )
                })
                .map(Some)?,
        }
    } else {
        None
    };

    let max_compressed = state.config.hosting.max_upload_bytes;
    let entries = tokio::task::spawn_blocking(move || extract_entries(&archive, max_compressed))
        .await
        .map_err(|e| anyhow::anyhow!("zip extract join: {e}"))??;

    let handle = state.storage.begin_staging().await?;
    for (rel_path, bytes) in entries {
        state
            .storage
            .put_staged(&handle, &rel_path, bytes)
            .await?;
    }

    let PromotedInfo {
        size_bytes,
        file_count,
    } = state.storage.promote(handle, &page_uuid).await?;

    let now = unix_now();
    let saved = if let Some(prev) = existing {
        let mut next = prev;
        next.name = form.name.clone().or(next.name);
        next.mode = mode;
        next.visibility = visibility;
        next.password_hash = password_hash;
        next.size_bytes = size_bytes;
        next.file_count = file_count;
        next.updated_at = now;
        state.repo.update_page(next).await?
    } else {
        state
            .repo
            .create_page(NewPage {
                uuid: page_uuid.clone(),
                name: form.name.clone(),
                mode,
                visibility,
                password_hash,
                owner_kind: OWNER_KIND_LOCAL.into(),
                owner_id: OWNER_ID_LOCAL.into(),
                size_bytes,
                file_count,
                created_at: now,
                updated_at: now,
            })
            .await?
    };

    let details = serde_json::json!({
        "size_bytes": size_bytes,
        "file_count": file_count,
        "mode": mode.as_str(),
        "visibility": visibility.as_str(),
    })
    .to_string();
    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(
                now,
                claims.sub.clone(),
                if updating {
                    AuditAction::PageUpdate
                } else {
                    AuditAction::PageCreate
                },
            )
            .with_target(page_uuid.clone())
            .with_scope(claims.scope.clone())
            .with_details(details),
        )
        .await;

    let url = format!(
        "{}/p/{}",
        state.config.server.public_url.trim_end_matches('/'),
        saved.uuid
    );

    Ok(Json(DeployResponse {
        uuid: saved.uuid,
        url,
        size_bytes,
        file_count,
        mode: mode.as_str().into(),
        visibility: visibility.as_str().into(),
    }))
}

async fn read_form(mut multipart: Multipart, max_archive: u64) -> Result<Form, ApiError> {
    let mut form = Form::default();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request("invalid_multipart", format!("multipart: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "archive" => {
                let bytes = field.bytes().await.map_err(|e| {
                    if format!("{e}").contains("length limit") {
                        ApiError::new(
                            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                            "archive_too_large",
                            format!("archive exceeds the upload size limit ({max_archive} bytes)"),
                        )
                    } else {
                        ApiError::bad_request(
                            "invalid_archive",
                            format!("reading archive field: {e}"),
                        )
                    }
                })?;
                if bytes.len() as u64 > max_archive {
                    return Err(ApiError::new(
                        axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                        "archive_too_large",
                        format!("archive {} bytes > limit {max_archive}", bytes.len()),
                    ));
                }
                form.archive = Some(bytes);
            }
            "uuid" => form.uuid = Some(text(field).await?),
            "mode" => form.mode = Some(text(field).await?),
            "name" => form.name = Some(text(field).await?),
            "visibility" => form.visibility = Some(text(field).await?),
            "password" => form.password = Some(text(field).await?),
            _ => {
                // Drain unknown fields so the underlying stream is consumed.
                let _ = field.bytes().await;
            }
        }
    }
    Ok(form)
}

async fn text(field: axum::extract::multipart::Field<'_>) -> Result<String, ApiError> {
    field
        .text()
        .await
        .map_err(|e| ApiError::bad_request("invalid_multipart", format!("reading field: {e}")))
}

/// Decompress a zip archive into a list of `(relative_path, bytes)` while
/// rejecting symlinks, directories outside the archive root, and anything
/// that would expand to absurd size. Returns the entries; the caller writes
/// them to storage. Strictly synchronous so it can run inside
/// `spawn_blocking`.
fn extract_entries(archive: &Bytes, max_compressed: u64) -> Result<Vec<(String, Bytes)>, ApiError> {
    let cursor = Cursor::new(archive.as_ref());
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| {
        ApiError::bad_request("invalid_archive", format!("not a valid zip: {e}"))
    })?;

    let mut out: Vec<(String, Bytes)> = Vec::new();
    let mut total_decompressed: u64 = 0;
    let decompressed_cap = max_compressed.saturating_mul(ZIP_BOMB_MULTIPLIER);

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| {
            ApiError::bad_request(
                "invalid_archive",
                format!("reading zip entry {i}: {e}"),
            )
        })?;

        if entry.is_dir() {
            continue;
        }

        if entry.is_symlink() {
            return Err(ApiError::new(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "symlink_in_archive",
                format!("symlinks are not allowed: {}", entry.name()),
            ));
        }

        // `enclosed_name` already rejects absolute paths and `..` traversal.
        let path = match entry.enclosed_name() {
            Some(p) => p,
            None => {
                return Err(ApiError::new(
                    axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    "path_traversal",
                    format!("entry path escapes archive root: {}", entry.name()),
                ));
            }
        };

        // Skip macOS metadata + DS_Store sidecars regardless of where they
        // appear in the tree.
        let mut skip = false;
        for component in path.iter() {
            let s = component.to_string_lossy();
            if s.starts_with("__MACOSX") || s == ".DS_Store" {
                skip = true;
                break;
            }
        }
        if skip {
            continue;
        }

        let rel = path
            .to_str()
            .ok_or_else(|| {
                ApiError::new(
                    axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_archive",
                    "entry path is not valid UTF-8",
                )
            })?
            .to_string();

        if out.len() >= MAX_FILES_PER_ARCHIVE {
            return Err(ApiError::new(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "too_many_files",
                format!(
                    "archive contains more than {MAX_FILES_PER_ARCHIVE} files; refusing"
                ),
            ));
        }

        let mut buf: Vec<u8> = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf).map_err(|e| {
            ApiError::bad_request("invalid_archive", format!("decompressing {rel}: {e}"))
        })?;

        total_decompressed = total_decompressed.saturating_add(buf.len() as u64);
        if total_decompressed > decompressed_cap {
            return Err(ApiError::new(
                axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                "archive_too_large",
                format!(
                    "decompressed archive exceeds {decompressed_cap} bytes (zip-bomb guard)"
                ),
            ));
        }

        out.push((rel, Bytes::from(buf)));
    }

    Ok(out)
}
