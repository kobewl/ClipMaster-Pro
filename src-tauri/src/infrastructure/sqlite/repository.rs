//! SQLite 实现的 ClipboardRepository。
//!
//! 并发规则（架构文档第 8 节）：
//! - SQLite 写入由固定连接管理，这里用 `Arc<Mutex<Connection>>` 承担该角色。
//! - 不跨 `await` 持有普通互斥锁：所有数据库操作放入 `spawn_blocking`，
//!   在同步闭包内部完成加锁、执行、释放，锁不会跨越 await 点。

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::error::RepositoryError;
use crate::domain::model::{ClipboardItem, ClipboardItemId, ContentType, NewClipboardItem};
use crate::domain::normalize::build_search_text;
use crate::domain::ports::{ClipboardRepository, DeleteResult, ListResult, SearchQuery};

pub struct SqliteClipboardRepository {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteClipboardRepository {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    fn map_db_err(err: rusqlite::Error) -> RepositoryError {
        RepositoryError::Database(err.to_string())
    }

    fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardItem> {
        let id_str: String = row.get("id")?;
        let content_type_str: String = row.get("content_type")?;
        let created_at_str: String = row.get("created_at")?;
        let updated_at_str: String = row.get("updated_at")?;
        let last_copied_at_str: String = row.get("last_copied_at")?;
        let is_favorite: i64 = row.get("is_favorite")?;

        Ok(ClipboardItem {
            id: ClipboardItemId::from_str(&id_str).unwrap_or_default(),
            content_type: ContentType::from_str(&content_type_str)
                .unwrap_or(ContentType::Text),
            content_text: row.get("content_text")?,
            fingerprint: row.get("fingerprint")?,
            is_favorite: is_favorite != 0,
            created_at: parse_datetime(&created_at_str),
            updated_at: parse_datetime(&updated_at_str),
            last_copied_at: parse_datetime(&last_copied_at_str),
            source_app: row.get("source_app")?,
            legacy_id: row.get("legacy_id")?,
        })
    }
}

fn parse_datetime(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[async_trait]
impl ClipboardRepository for SqliteClipboardRepository {
    async fn insert_or_touch(
        &self,
        item: NewClipboardItem,
    ) -> Result<ClipboardItem, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let now = Utc::now().to_rfc3339();

            // 去重策略（D-002）：已存在相同 fingerprint 时，更新时间并保留收藏状态，
            // 不做 v2 那种 delete+insert，从而不丢失收藏/不改变稳定 id。
            let existing_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM clipboard_items WHERE fingerprint = ?1",
                    params![item.fingerprint],
                    |row| row.get(0),
                )
                .optional()
                .map_err(Self::map_db_err)?;

            let search_text = build_search_text(&item.content_text);

            if let Some(existing_id) = existing_id {
                conn.execute(
                    "UPDATE clipboard_items
                     SET updated_at = ?1, last_copied_at = ?1, source_app = ?2
                     WHERE id = ?3",
                    params![now, item.source_app, existing_id],
                )
                .map_err(Self::map_db_err)?;

                return conn
                    .query_row(
                        "SELECT * FROM clipboard_items WHERE id = ?1",
                        params![existing_id],
                        Self::row_to_item,
                    )
                    .map_err(Self::map_db_err);
            }

            let new_id = ClipboardItemId::new();
            conn.execute(
                "INSERT INTO clipboard_items
                    (id, content_type, content_text, fingerprint, search_text,
                     is_favorite, created_at, updated_at, last_copied_at, source_app, legacy_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6, ?6, ?7, NULL)",
                params![
                    new_id.to_string(),
                    item.content_type.as_str(),
                    item.content_text,
                    item.fingerprint,
                    search_text,
                    now,
                    item.source_app,
                ],
            )
            .map_err(Self::map_db_err)?;

            conn.query_row(
                "SELECT * FROM clipboard_items WHERE id = ?1",
                params![new_id.to_string()],
                Self::row_to_item,
            )
            .map_err(Self::map_db_err)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn search(&self, query: SearchQuery) -> Result<ListResult, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");

            let search_pattern = query
                .search_text
                .as_ref()
                .map(|s| format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));

            let mut where_clauses: Vec<&str> = Vec::new();
            if query.favorites_only {
                where_clauses.push("is_favorite = 1");
            }
            if search_pattern.is_some() {
                where_clauses.push("search_text LIKE ?1 ESCAPE '\\'");
            }
            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", where_clauses.join(" AND "))
            };

            let count_sql = format!("SELECT COUNT(*) FROM clipboard_items {where_sql}");
            let total: i64 = if let Some(ref pattern) = search_pattern {
                conn.query_row(&count_sql, params![pattern], |row| row.get(0))
            } else {
                conn.query_row(&count_sql, [], |row| row.get(0))
            }
            .map_err(Self::map_db_err)?;

            let list_sql = format!(
                "SELECT * FROM clipboard_items {where_sql}
                 ORDER BY is_favorite DESC, last_copied_at DESC
                 LIMIT ?{} OFFSET ?{}",
                if search_pattern.is_some() { 2 } else { 1 },
                if search_pattern.is_some() { 3 } else { 2 },
            );

            let mut items = Vec::new();
            if let Some(ref pattern) = search_pattern {
                let mut stmt = conn.prepare(&list_sql).map_err(Self::map_db_err)?;
                let mut rows = stmt
                    .query(params![pattern, query.limit, query.offset])
                    .map_err(Self::map_db_err)?;
                while let Some(row) = rows.next().map_err(Self::map_db_err)? {
                    items.push(Self::row_to_item(row).map_err(Self::map_db_err)?);
                }
            } else {
                let mut stmt = conn.prepare(&list_sql).map_err(Self::map_db_err)?;
                let mut rows = stmt
                    .query(params![query.limit, query.offset])
                    .map_err(Self::map_db_err)?;
                while let Some(row) = rows.next().map_err(Self::map_db_err)? {
                    items.push(Self::row_to_item(row).map_err(Self::map_db_err)?);
                }
            }

            Ok(ListResult {
                items,
                total: total as u64,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn get_by_id(&self, id: ClipboardItemId) -> Result<ClipboardItem, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            conn.query_row(
                "SELECT * FROM clipboard_items WHERE id = ?1",
                params![id.to_string()],
                Self::row_to_item,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    RepositoryError::NotFound(id.to_string())
                }
                other => Self::map_db_err(other),
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn set_favorite(
        &self,
        id: ClipboardItemId,
        favorite: bool,
    ) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let now = Utc::now().to_rfc3339();
            let affected = conn
                .execute(
                    "UPDATE clipboard_items SET is_favorite = ?1, updated_at = ?2 WHERE id = ?3",
                    params![favorite as i64, now, id.to_string()],
                )
                .map_err(Self::map_db_err)?;
            if affected == 0 {
                return Err(RepositoryError::NotFound(id.to_string()));
            }
            Ok(())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn delete(&self, id: ClipboardItemId) -> Result<DeleteResult, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let affected = conn
                .execute(
                    "DELETE FROM clipboard_items WHERE id = ?1",
                    params![id.to_string()],
                )
                .map_err(Self::map_db_err)?;
            Ok(DeleteResult {
                deleted: affected > 0,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn clear(&self, keep_favorites: bool) -> Result<u64, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let sql = if keep_favorites {
                "DELETE FROM clipboard_items WHERE is_favorite = 0"
            } else {
                "DELETE FROM clipboard_items"
            };
            let affected = conn.execute(sql, []).map_err(Self::map_db_err)?;
            Ok(affected as u64)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn enforce_max_count(&self, max_count: u32) -> Result<u64, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            // 收藏项永远不计入上限、不被本操作删除（FR-MGT-006）。
            let affected = conn
                .execute(
                    "DELETE FROM clipboard_items
                     WHERE is_favorite = 0
                       AND id IN (
                         SELECT id FROM clipboard_items
                         WHERE is_favorite = 0
                         ORDER BY last_copied_at ASC
                         LIMIT MAX(
                           0,
                           (SELECT COUNT(*) FROM clipboard_items WHERE is_favorite = 0) - ?1
                         )
                       )",
                    params![max_count],
                )
                .map_err(Self::map_db_err)?;
            Ok(affected as u64)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn enforce_retention_days(&self, retention_days: i64) -> Result<u64, RepositoryError> {
        if retention_days <= 0 {
            return Ok(0);
        }
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let cutoff = (Utc::now() - chrono::Duration::days(retention_days)).to_rfc3339();
            let affected = conn
                .execute(
                    "DELETE FROM clipboard_items
                     WHERE is_favorite = 0 AND last_copied_at < ?1",
                    params![cutoff],
                )
                .map_err(Self::map_db_err)?;
            Ok(affected as u64)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}
