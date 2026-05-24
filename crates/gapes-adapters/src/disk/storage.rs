use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use gapes_core::error::{CoreError, Result as CoreResult};
use gapes_core::ids::IdGen;
use gapes_core::ports::{PromotedInfo, StagingHandle, Storage};

pub struct DiskStorage {
    pages_dir: PathBuf,
    trash_dir: PathBuf,
    staging_dir: PathBuf,
    id_gen: Arc<dyn IdGen>,
}

impl DiskStorage {
    pub fn new(
        pages_dir: PathBuf,
        trash_dir: PathBuf,
        staging_dir: PathBuf,
        id_gen: Arc<dyn IdGen>,
    ) -> Self {
        Self {
            pages_dir,
            trash_dir,
            staging_dir,
            id_gen,
        }
    }

    fn staging_path(&self, handle: &StagingHandle) -> PathBuf {
        self.staging_dir.join(&handle.id)
    }

    fn page_path(&self, page_uuid: &str) -> PathBuf {
        self.pages_dir.join(page_uuid)
    }
}

#[async_trait]
impl Storage for DiskStorage {
    async fn begin_staging(&self) -> CoreResult<StagingHandle> {
        let id = self.id_gen.new_ulid().to_string();
        let dir = self.staging_dir.join(&id);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| CoreError::StorageFailure(format!("create staging dir: {e}")))?;
        Ok(StagingHandle { id })
    }

    async fn put_staged(&self, h: &StagingHandle, rel_path: &str, bytes: Bytes) -> CoreResult<()> {
        let base = self.staging_path(h);
        let target = safe_join(&base, rel_path)
            .map_err(|e| CoreError::InvalidInput(format!("staging put: {e}")))?;

        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| CoreError::StorageFailure(format!("create parent: {e}")))?;
        }

        write_regular_file(&target, &bytes)
            .await
            .map_err(|e| CoreError::StorageFailure(format!("write staged file: {e}")))?;
        Ok(())
    }

    async fn promote(&self, h: StagingHandle, page_uuid: &str) -> CoreResult<PromotedInfo> {
        let staging = self.staging_path(&h);
        let target = self.page_path(page_uuid);

        if !tokio::fs::try_exists(&staging).await.unwrap_or(false) {
            return Err(CoreError::StorageFailure(format!(
                "staging dir missing: {}",
                staging.display()
            )));
        }

        tokio::fs::create_dir_all(&self.pages_dir)
            .await
            .map_err(|e| CoreError::StorageFailure(format!("create pages dir: {e}")))?;
        tokio::fs::create_dir_all(&self.trash_dir)
            .await
            .map_err(|e| CoreError::StorageFailure(format!("create trash dir: {e}")))?;

        if tokio::fs::try_exists(&target).await.unwrap_or(false) {
            let trash_target = self.trash_dir.join(format!("{}-{}", page_uuid, unix_now()));
            tokio::fs::rename(&target, &trash_target)
                .await
                .map_err(|e| CoreError::StorageFailure(format!("move old to trash: {e}")))?;
        }

        tokio::fs::rename(&staging, &target)
            .await
            .map_err(|e| CoreError::StorageFailure(format!("promote staging: {e}")))?;

        let final_target = target.clone();
        let (size_bytes, file_count) = tokio::task::spawn_blocking(move || {
            walk_size_and_count(&final_target)
        })
        .await
        .map_err(|e| CoreError::StorageFailure(format!("walk join: {e}")))??;

        Ok(PromotedInfo {
            size_bytes,
            file_count,
        })
    }

    async fn read(&self, page_uuid: &str, rel_path: &str) -> CoreResult<Option<Bytes>> {
        let base = self.page_path(page_uuid);
        let target = match safe_join(&base, rel_path) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        match tokio::fs::read(&target).await {
            Ok(v) => Ok(Some(Bytes::from(v))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CoreError::StorageFailure(format!("read: {e}"))),
        }
    }

    async fn delete_page(&self, page_uuid: &str) -> CoreResult<()> {
        let target = self.page_path(page_uuid);
        if !tokio::fs::try_exists(&target).await.unwrap_or(false) {
            return Ok(());
        }
        tokio::fs::create_dir_all(&self.trash_dir)
            .await
            .map_err(|e| CoreError::StorageFailure(format!("create trash dir: {e}")))?;
        let trash_target = self.trash_dir.join(format!("{}-{}", page_uuid, unix_now()));
        tokio::fs::rename(&target, &trash_target)
            .await
            .map_err(|e| CoreError::StorageFailure(format!("move to trash: {e}")))?;
        Ok(())
    }
}

/// Join `base` with `rel` while rejecting path traversal / absolute paths /
/// anything that would escape `base`. The returned path is `base` + the
/// normalised components of `rel`.
fn safe_join(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = Path::new(rel);
    if rel.is_absolute() {
        return Err("absolute paths not allowed".into());
    }
    let mut out = base.to_path_buf();
    for comp in rel.components() {
        match comp {
            Component::Normal(seg) => out.push(seg),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("disallowed path component in {:?}", rel));
            }
        }
    }
    Ok(out)
}

#[cfg(unix)]
async fn write_regular_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut f = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(path)
        .await?;
    f.write_all(bytes).await?;
    f.flush().await?;
    Ok(())
}

#[cfg(not(unix))]
async fn write_regular_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut f = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await?;
    f.write_all(bytes).await?;
    f.flush().await?;
    Ok(())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Walk a directory and sum file count + total bytes. Synchronous so we can
/// run inside `spawn_blocking`; `walkdir` already handles symlinks-as-files
/// and won't follow them by default.
fn walk_size_and_count(root: &Path) -> CoreResult<(u64, u64)> {
    let mut size = 0u64;
    let mut count = 0u64;
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| CoreError::StorageFailure(format!("walk: {e}")))?;
        if entry.file_type().is_file() {
            count += 1;
            if let Ok(meta) = entry.metadata() {
                size += meta.len();
            }
        }
    }
    Ok((size, count))
}
