use std::sync::Arc;

use async_trait::async_trait;
use gapes_core::audit::AuditEvent;
use gapes_core::comment::{Comment, CommentStatus, NewComment};
use gapes_core::error::{CoreError, Result};
use gapes_core::hit::NewHit;
use gapes_core::ids::IdGen;
use gapes_core::page::{NewPage, Page, PageStats};
use gapes_core::ports::Repository;
use gapes_core::time::Clock;
use sqlx::{Row, SqlitePool};

use super::mapping::{audit_from_row, comment_from_row, page_from_row};

pub struct SqliteRepository {
    pool: SqlitePool,
    #[allow(dead_code)]
    clock: Arc<dyn Clock>,
    #[allow(dead_code)]
    id_gen: Arc<dyn IdGen>,
}

impl SqliteRepository {
    pub fn new(pool: SqlitePool, clock: Arc<dyn Clock>, id_gen: Arc<dyn IdGen>) -> Self {
        Self {
            pool,
            clock,
            id_gen,
        }
    }
}

fn db<E: std::fmt::Display>(e: E) -> CoreError {
    CoreError::RepositoryFailure(e.to_string())
}

#[async_trait]
impl Repository for SqliteRepository {
    async fn create_page(&self, p: NewPage) -> Result<Page> {
        sqlx::query(
            r#"
            INSERT INTO pages (
                uuid, name, mode, visibility, password_hash,
                owner_kind, owner_id, size_bytes, file_count,
                comments_enabled, comments_require_approval,
                csp, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?, ?)
            "#,
        )
        .bind(&p.uuid)
        .bind(&p.name)
        .bind(p.mode.as_str())
        .bind(p.visibility.as_str())
        .bind(&p.password_hash)
        .bind(&p.owner_kind)
        .bind(&p.owner_id)
        .bind(p.size_bytes as i64)
        .bind(p.file_count as i64)
        .bind(p.csp.as_str())
        .bind(p.created_at)
        .bind(p.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db)?;

        self.find_page(&p.uuid)
            .await?
            .ok_or(CoreError::RepositoryFailure(
                "page disappeared after insert".into(),
            ))
    }

    async fn update_page(&self, p: Page) -> Result<Page> {
        let res = sqlx::query(
            r#"
            UPDATE pages SET
                name = ?, mode = ?, visibility = ?, password_hash = ?,
                owner_kind = ?, owner_id = ?, size_bytes = ?, file_count = ?,
                comments_enabled = ?, comments_require_approval = ?,
                csp = ?,
                updated_at = ?
            WHERE uuid = ?
            "#,
        )
        .bind(&p.name)
        .bind(p.mode.as_str())
        .bind(p.visibility.as_str())
        .bind(&p.password_hash)
        .bind(&p.owner_kind)
        .bind(&p.owner_id)
        .bind(p.size_bytes as i64)
        .bind(p.file_count as i64)
        .bind(p.comments_enabled as i64)
        .bind(p.comments_require_approval as i64)
        .bind(p.csp.as_str())
        .bind(p.updated_at)
        .bind(&p.uuid)
        .execute(&self.pool)
        .await
        .map_err(db)?;

        if res.rows_affected() == 0 {
            return Err(CoreError::NotFound);
        }
        self.find_page(&p.uuid).await?.ok_or(CoreError::NotFound)
    }

    async fn set_page_archived(&self, uuid: &str, archived: bool) -> Result<()> {
        let now = self.clock.now();
        let res = sqlx::query(
            "UPDATE pages SET archived = ?, updated_at = ? WHERE uuid = ?",
        )
        .bind(archived as i64)
        .bind(now)
        .bind(uuid)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if res.rows_affected() == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }

    async fn find_page(&self, uuid: &str) -> Result<Option<Page>> {
        let row = sqlx::query("SELECT * FROM pages WHERE uuid = ?")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        match row {
            Some(r) => Ok(Some(page_from_row(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_pages(&self, owner_kind: &str, owner_id: &str) -> Result<Vec<Page>> {
        let rows = sqlx::query(
            "SELECT * FROM pages WHERE owner_kind = ? AND owner_id = ? ORDER BY created_at DESC",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(page_from_row).collect()
    }

    async fn delete_page(&self, uuid: &str) -> Result<()> {
        let res = sqlx::query("DELETE FROM pages WHERE uuid = ?")
            .bind(uuid)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        if res.rows_affected() == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }

    async fn record_hit(&self, h: NewHit) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO hits (page_uuid, ts, ip_hash, ua_hash, path, referrer, status)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&h.page_uuid)
        .bind(h.ts)
        .bind(&h.ip_hash)
        .bind(&h.ua_hash)
        .bind(&h.path)
        .bind(&h.referrer)
        .bind(h.status as i64)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn stats(&self, page_uuid: &str, since_ts: i64) -> Result<PageStats> {
        let views: i64 =
            sqlx::query("SELECT COUNT(*) AS c FROM hits WHERE page_uuid = ? AND ts >= ?")
                .bind(page_uuid)
                .bind(since_ts)
                .fetch_one(&self.pool)
                .await
                .map_err(db)?
                .try_get("c")
                .map_err(db)?;

        let uniques: i64 = sqlx::query(
            "SELECT COUNT(DISTINCT ip_hash) AS c FROM hits WHERE page_uuid = ? AND ts >= ?",
        )
        .bind(page_uuid)
        .bind(since_ts)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?
        .try_get("c")
        .map_err(db)?;

        let path_rows = sqlx::query(
            r#"
            SELECT path AS p, COUNT(*) AS c
            FROM hits
            WHERE page_uuid = ? AND ts >= ?
            GROUP BY path
            ORDER BY c DESC, p ASC
            LIMIT 10
            "#,
        )
        .bind(page_uuid)
        .bind(since_ts)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let mut top_paths = Vec::with_capacity(path_rows.len());
        for r in &path_rows {
            let p: String = r.try_get("p").map_err(db)?;
            let c: i64 = r.try_get("c").map_err(db)?;
            top_paths.push((p, c as u64));
        }

        let ref_rows = sqlx::query(
            r#"
            SELECT COALESCE(referrer, '(direct)') AS r, COUNT(*) AS c
            FROM hits
            WHERE page_uuid = ? AND ts >= ?
            GROUP BY COALESCE(referrer, '(direct)')
            ORDER BY c DESC, r ASC
            LIMIT 10
            "#,
        )
        .bind(page_uuid)
        .bind(since_ts)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        let mut top_referrers = Vec::with_capacity(ref_rows.len());
        for r in &ref_rows {
            let referrer: String = r.try_get("r").map_err(db)?;
            let c: i64 = r.try_get("c").map_err(db)?;
            top_referrers.push((referrer, c as u64));
        }

        Ok(PageStats {
            views: views as u64,
            uniques: uniques as u64,
            top_paths,
            top_referrers,
        })
    }

    async fn enable_comments(
        &self,
        page_uuid: &str,
        enabled: bool,
        require_approval: bool,
    ) -> Result<()> {
        let res = sqlx::query(
            r#"
            UPDATE pages
            SET comments_enabled = ?, comments_require_approval = ?
            WHERE uuid = ?
            "#,
        )
        .bind(enabled as i64)
        .bind(require_approval as i64)
        .bind(page_uuid)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if res.rows_affected() == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }

    async fn create_comment(&self, c: NewComment) -> Result<Comment> {
        sqlx::query(
            r#"
            INSERT INTO comments (
                id, page_uuid, author, body_md, body_html,
                contact, ts, ip_hash, status, user_agent,
                anchor_selector, anchor_text, anchor_offset
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&c.id)
        .bind(&c.page_uuid)
        .bind(&c.author)
        .bind(&c.body_md)
        .bind(&c.body_html)
        .bind(&c.contact)
        .bind(c.ts)
        .bind(&c.ip_hash)
        .bind(c.status.as_str())
        .bind(&c.user_agent)
        .bind(&c.anchor_selector)
        .bind(&c.anchor_text)
        .bind(c.anchor_offset)
        .execute(&self.pool)
        .await
        .map_err(db)?;

        self.find_comment(&c.id).await?.ok_or(CoreError::RepositoryFailure(
            "comment disappeared after insert".into(),
        ))
    }

    async fn list_comments(&self, page_uuid: &str, include_hidden: bool) -> Result<Vec<Comment>> {
        let rows = if include_hidden {
            sqlx::query("SELECT * FROM comments WHERE page_uuid = ? ORDER BY ts ASC")
                .bind(page_uuid)
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query(
                "SELECT * FROM comments WHERE page_uuid = ? AND status = 'visible' ORDER BY ts ASC",
            )
            .bind(page_uuid)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(db)?;
        rows.iter().map(comment_from_row).collect()
    }

    async fn find_comment(&self, id: &str) -> Result<Option<Comment>> {
        let row = sqlx::query("SELECT * FROM comments WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        match row {
            Some(r) => Ok(Some(comment_from_row(&r)?)),
            None => Ok(None),
        }
    }

    async fn set_comment_status(&self, id: &str, status: CommentStatus) -> Result<()> {
        let res = sqlx::query("UPDATE comments SET status = ? WHERE id = ?")
            .bind(status.as_str())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        if res.rows_affected() == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }

    async fn delete_comment(&self, id: &str) -> Result<()> {
        let res = sqlx::query("DELETE FROM comments WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        if res.rows_affected() == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }

    async fn count_recent_comments(
        &self,
        page_uuid: &str,
        ip_hash: &str,
        since_ts: i64,
    ) -> Result<u64> {
        let n: i64 = sqlx::query(
            "SELECT COUNT(*) AS c FROM comments WHERE page_uuid = ? AND ip_hash = ? AND ts >= ?",
        )
        .bind(page_uuid)
        .bind(ip_hash)
        .bind(since_ts)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?
        .try_get("c")
        .map_err(db)?;
        Ok(n as u64)
    }

    async fn count_recent_comments_server(&self, ip_hash: &str, since_ts: i64) -> Result<u64> {
        let n: i64 = sqlx::query("SELECT COUNT(*) AS c FROM comments WHERE ip_hash = ? AND ts >= ?")
            .bind(ip_hash)
            .bind(since_ts)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?
            .try_get("c")
            .map_err(db)?;
        Ok(n as u64)
    }

    async fn record_audit(&self, e: AuditEvent) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO audit_events
                (ts, actor, ip_hash, scope, action, target, success, details)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(e.ts)
        .bind(&e.actor)
        .bind(&e.ip_hash)
        .bind(&e.scope)
        .bind(e.action.as_str())
        .bind(&e.target)
        .bind(e.success as i64)
        .bind(&e.details)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn recent_audit(&self, since_ts: i64) -> Result<Vec<AuditEvent>> {
        let rows =
            sqlx::query("SELECT * FROM audit_events WHERE ts >= ? ORDER BY ts DESC LIMIT 200")
                .bind(since_ts)
                .fetch_all(&self.pool)
                .await
                .map_err(db)?;
        rows.iter().map(audit_from_row).collect()
    }
}
