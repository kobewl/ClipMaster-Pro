//! SQLite 实现的 ClipboardRepository。
//!
//! 搜索使用 FTS5 全文索引（migration 2），性能远优于 LIKE '%xxx%'：
//! - B-tree 前缀索引对 '%keyword%' 无效，FTS5 的倒排索引天然支持任意位置匹配
//! - 后续可扩展：前缀搜索、多词搜索、排序等
//!
//! 并发规则（架构文档第 8 节）：
//! - 所有数据库操作放入 `spawn_blocking`，不跨 `await` 持有 Mutex。

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
            content_type: ContentType::from_str(&content_type_str).unwrap_or(ContentType::Text),
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

/// 同步 FTS5 索引：插入或更新后调用。
/// external content 模式下 FTS 不会自动同步，必须手动 INSERT/DELETE。
fn sync_fts_insert(conn: &Connection, rowid: i64, search_text: &str) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO clipboard_items_fts(rowid, search_text) VALUES (?1, ?2)",
        params![rowid, search_text],
    )
    .map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(())
}

fn sync_fts_delete(conn: &Connection, rowid: i64, old_search_text: &str) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO clipboard_items_fts(clipboard_items_fts, rowid, search_text) VALUES('delete', ?1, ?2)",
        params![rowid, old_search_text],
    )
    .map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(())
}

/// 获取一行的 rowid（SQLite 隐式行号）。
fn get_rowid(conn: &Connection, id: &str) -> Result<i64, RepositoryError> {
    conn.query_row(
        "SELECT rowid FROM clipboard_items WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )
    .map_err(|e| RepositoryError::Database(e.to_string()))
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

            let existing: Option<(String, String)> = conn
                .query_row(
                    "SELECT id, search_text FROM clipboard_items WHERE fingerprint = ?1",
                    params![item.fingerprint],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(Self::map_db_err)?;

            let search_text = build_search_text(&item.content_text);

            if let Some((existing_id, old_search_text)) = existing {
                conn.execute(
                    "UPDATE clipboard_items
                     SET updated_at = ?1, last_copied_at = ?1, source_app = ?2, search_text = ?3
                     WHERE id = ?4",
                    params![now, item.source_app, search_text, existing_id],
                )
                .map_err(Self::map_db_err)?;

                // FTS external content 模式：delete 旧的，insert 新的
                let rowid = get_rowid(&conn, &existing_id)?;
                let _ = sync_fts_delete(&conn, rowid, &old_search_text);
                let _ = sync_fts_insert(&conn, rowid, &search_text);

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

            // 同步 FTS 索引
            let rowid = get_rowid(&conn, &new_id.to_string())?;
            let _ = sync_fts_insert(&conn, rowid, &search_text);

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

            // FTS5 搜索：把用户输入转为 FTS5 MATCH 表达式
            // 例如 "hello world" → "hello* world*"（每个词加前缀通配）
            let fts_match_expr = query.search_text.as_ref().map(|s| {
                s.split_whitespace()
                    .map(|word| {
                        // 转义 FTS5 特殊字符
                        let escaped = word
                            .replace('"', "\"\"");
                        format!("\"{escaped}\"*")
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            });

            let use_fts = fts_match_expr.is_some();

            // 构建查询
            let (from_clause, search_condition) = if let Some(ref expr) = fts_match_expr {
                if expr.trim().is_empty() {
                    ("clipboard_items".to_string(), None)
                } else {
                    // FTS5 JOIN：通过 rowid 关联主表
                    (
                        "clipboard_items INNER JOIN clipboard_items_fts ON clipboard_items.rowid = clipboard_items_fts.rowid".to_string(),
                        Some(format!("clipboard_items_fts MATCH '{}'", expr.replace('\'', "''")))
                    )
                }
            } else {
                ("clipboard_items".to_string(), None)
            };

            let mut where_clauses: Vec<String> = Vec::new();
            if query.favorites_only {
                where_clauses.push("clipboard_items.is_favorite = 1".to_string());
            }
            if let Some(cond) = search_condition {
                where_clauses.push(cond);
            }
            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", where_clauses.join(" AND "))
            };

            // COUNT
            let count_sql =
                format!("SELECT COUNT(*) FROM {from_clause} {where_sql}");
            let total: i64 = conn
                .query_row(&count_sql, [], |row| row.get(0))
                .map_err(Self::map_db_err)?;

            // SELECT with pagination
            let list_sql = format!(
                "SELECT clipboard_items.* FROM {from_clause} {where_sql}
                 ORDER BY clipboard_items.is_favorite DESC, clipboard_items.last_copied_at DESC
                 LIMIT ?1 OFFSET ?2"
            );

            let mut stmt = conn.prepare(&list_sql).map_err(Self::map_db_err)?;
            let mut rows = stmt
                .query(params![query.limit, query.offset])
                .map_err(Self::map_db_err)?;
            let mut items = Vec::new();
            while let Some(row) = rows.next().map_err(Self::map_db_err)? {
                items.push(Self::row_to_item(row).map_err(Self::map_db_err)?);
            }

            let _ = use_fts; // suppress unused warning

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

            // 先读 FTS 需要的数据再删除
            let fts_data: Option<(i64, String)> = conn
                .query_row(
                    "SELECT rowid, search_text FROM clipboard_items WHERE id = ?1",
                    params![id.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(Self::map_db_err)?;

            let affected = conn
                .execute(
                    "DELETE FROM clipboard_items WHERE id = ?1",
                    params![id.to_string()],
                )
                .map_err(Self::map_db_err)?;

            // 同步删除 FTS 索引
            if let Some((rowid, search_text)) = fts_data {
                let _ = sync_fts_delete(&conn, rowid, &search_text);
            }

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

            // 批量操作后直接 rebuild FTS 索引比逐条 delete 更高效
            conn.execute(
                "INSERT INTO clipboard_items_fts(clipboard_items_fts) VALUES('rebuild')",
                [],
            )
            .map_err(Self::map_db_err)?;

            Ok(affected as u64)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn enforce_max_count(&self, max_count: u32) -> Result<u64, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
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

            if affected > 0 {
                // 批量删除后 rebuild
                let _ = conn.execute(
                    "INSERT INTO clipboard_items_fts(clipboard_items_fts) VALUES('rebuild')",
                    [],
                );
            }

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

            if affected > 0 {
                let _ = conn.execute(
                    "INSERT INTO clipboard_items_fts(clipboard_items_fts) VALUES('rebuild')",
                    [],
                );
            }

            Ok(affected as u64)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}
