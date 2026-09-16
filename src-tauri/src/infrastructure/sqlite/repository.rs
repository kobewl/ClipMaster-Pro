//! SQLite 实现的 ClipboardRepository。
//!
//! 搜索使用 FTS5 + trigram tokenizer（migration 3），天然支持 CJK 子串匹配：
//! - ≥3 字符查询走 FTS5 MATCH（trigram 子串索引）
//! - <3 字符查询自动 fallback 到 LIKE（短词 FTS 无法形成有效 trigram）
//!
//! 事务规则：所有涉及"主表 + FTS 索引"的写操作必须在同一事务内完成，
//! FTS 同步失败会回滚整个事务，杜绝 index drift。
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
use crate::domain::ports::{
    CleanupResult, ClipboardRepository, DeleteResult, ListResult, SearchQuery,
};

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

// ---------------------------------------------------------------------------
//  FTS5 同步辅助函数
//  external content 模式下 FTS 不会自动同步，必须手动 INSERT/DELETE。
//  这些函数的错误必须向上传播，由调用方在事务中处理，绝不能 `let _ =`。
// ---------------------------------------------------------------------------

fn sync_fts_insert(
    conn: &Connection,
    rowid: i64,
    search_text: &str,
) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO clipboard_items_fts(rowid, search_text) VALUES (?1, ?2)",
        params![rowid, search_text],
    )
    .map_err(|e| RepositoryError::Database(format!("FTS insert failed: {e}")))?;
    Ok(())
}

fn sync_fts_delete(
    conn: &Connection,
    rowid: i64,
    old_search_text: &str,
) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO clipboard_items_fts(clipboard_items_fts, rowid, search_text) \
         VALUES('delete', ?1, ?2)",
        params![rowid, old_search_text],
    )
    .map_err(|e| RepositoryError::Database(format!("FTS delete failed: {e}")))?;
    Ok(())
}

fn fts_rebuild(conn: &Connection) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO clipboard_items_fts(clipboard_items_fts) VALUES('rebuild')",
        [],
    )
    .map_err(|e| RepositoryError::Database(format!("FTS rebuild failed: {e}")))?;
    Ok(())
}

fn get_rowid(conn: &Connection, id: &str) -> Result<i64, RepositoryError> {
    conn.query_row(
        "SELECT rowid FROM clipboard_items WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )
    .map_err(|e| RepositoryError::Database(e.to_string()))
}

/// 查询指定条件下将被删除的图片文件路径。
fn collect_image_paths_with_sql(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
) -> Result<Vec<String>, RepositoryError> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
    let paths: Vec<String> = stmt
        .query_map(params, |row| row.get(0))
        .map_err(|e| RepositoryError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(paths)
}

#[async_trait]
impl ClipboardRepository for SqliteClipboardRepository {
    async fn insert_or_touch(
        &self,
        item: NewClipboardItem,
    ) -> Result<ClipboardItem, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let now = Utc::now().to_rfc3339();
            let search_text = build_search_text(&item.content_text);

            let tx = conn.transaction().map_err(Self::map_db_err)?;

            let existing: Option<(String, String)> = tx
                .query_row(
                    "SELECT id, search_text FROM clipboard_items WHERE fingerprint = ?1",
                    params![item.fingerprint],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(Self::map_db_err)?;

            if let Some((existing_id, old_search_text)) = existing {
                tx.execute(
                    "UPDATE clipboard_items
                     SET updated_at = ?1, last_copied_at = ?1, source_app = ?2, search_text = ?3
                     WHERE id = ?4",
                    params![now, item.source_app, search_text, existing_id],
                )
                .map_err(Self::map_db_err)?;

                let rowid = get_rowid(&tx, &existing_id)?;
                sync_fts_delete(&tx, rowid, &old_search_text)?;
                sync_fts_insert(&tx, rowid, &search_text)?;

                let result = tx
                    .query_row(
                        "SELECT * FROM clipboard_items WHERE id = ?1",
                        params![existing_id],
                        Self::row_to_item,
                    )
                    .map_err(Self::map_db_err)?;

                tx.commit().map_err(Self::map_db_err)?;
                return Ok(result);
            }

            let new_id = ClipboardItemId::new();
            tx.execute(
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

            let rowid = get_rowid(&tx, &new_id.to_string())?;
            sync_fts_insert(&tx, rowid, &search_text)?;

            let result = tx
                .query_row(
                    "SELECT * FROM clipboard_items WHERE id = ?1",
                    params![new_id.to_string()],
                    Self::row_to_item,
                )
                .map_err(Self::map_db_err)?;

            tx.commit().map_err(Self::map_db_err)?;
            Ok(result)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn search(&self, query: SearchQuery) -> Result<ListResult, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");

            // ----------------------------------------------------------
            // 搜索策略（trigram tokenizer, migration 3）：
            //   ≥3 字符的词 → FTS5 MATCH（trigram 子串匹配）
            //   <3 字符的词 → LIKE fallback（trigram 至少需要 3 字符）
            //   混合时 FTS + LIKE 联合 AND 过滤
            // ----------------------------------------------------------
            let mut fts_terms: Vec<String> = Vec::new();
            let mut like_patterns: Vec<String> = Vec::new();

            if let Some(ref text) = query.search_text {
                for word in text.split_whitespace() {
                    if word.is_empty() {
                        continue;
                    }
                    if word.chars().count() >= 3 {
                        let escaped = word.replace('"', "\"\"");
                        fts_terms.push(format!("\"{escaped}\""));
                    } else {
                        let escaped = word
                            .replace('\\', "\\\\")
                            .replace('%', "\\%")
                            .replace('_', "\\_");
                        like_patterns.push(format!("%{escaped}%"));
                    }
                }
            }

            let use_fts = !fts_terms.is_empty();
            let fts_expr = fts_terms.join(" ");

            let from_clause = if use_fts {
                "clipboard_items INNER JOIN clipboard_items_fts \
                 ON clipboard_items.rowid = clipboard_items_fts.rowid"
            } else {
                "clipboard_items"
            };

            // 动态构建 WHERE 条件和参数列表
            let mut conditions: Vec<String> = Vec::new();
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut pidx = 0usize;

            if query.favorites_only {
                conditions.push("clipboard_items.is_favorite = 1".to_string());
            }

            if use_fts {
                pidx += 1;
                conditions.push(format!("clipboard_items_fts MATCH ?{pidx}"));
                param_values.push(Box::new(fts_expr));
            }

            for pattern in &like_patterns {
                pidx += 1;
                conditions.push(format!(
                    "clipboard_items.search_text LIKE ?{pidx} ESCAPE '\\'"
                ));
                param_values.push(Box::new(pattern.clone()));
            }

            let where_sql = if conditions.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", conditions.join(" AND "))
            };

            // COUNT
            let count_sql = format!("SELECT COUNT(*) FROM {from_clause} {where_sql}");
            let count_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();
            let total: i64 = conn
                .query_row(&count_sql, count_refs.as_slice(), |row| row.get(0))
                .map_err(Self::map_db_err)?;

            // SELECT with pagination
            pidx += 1;
            let limit_idx = pidx;
            pidx += 1;
            let offset_idx = pidx;
            let list_sql = format!(
                "SELECT clipboard_items.* FROM {from_clause} {where_sql}
                 ORDER BY clipboard_items.is_favorite DESC, clipboard_items.last_copied_at DESC
                 LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
            );

            param_values.push(Box::new(query.limit));
            param_values.push(Box::new(query.offset));
            let list_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            let mut stmt = conn.prepare(&list_sql).map_err(Self::map_db_err)?;
            let mut rows = stmt
                .query(list_refs.as_slice())
                .map_err(Self::map_db_err)?;
            let mut items = Vec::new();
            while let Some(row) = rows.next().map_err(Self::map_db_err)? {
                items.push(Self::row_to_item(row).map_err(Self::map_db_err)?);
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
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn.transaction().map_err(Self::map_db_err)?;

            let item_data: Option<(i64, String, String, String)> = tx
                .query_row(
                    "SELECT rowid, search_text, content_type, content_text \
                     FROM clipboard_items WHERE id = ?1",
                    params![id.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(Self::map_db_err)?;

            let affected = tx
                .execute(
                    "DELETE FROM clipboard_items WHERE id = ?1",
                    params![id.to_string()],
                )
                .map_err(Self::map_db_err)?;

            let mut image_path: Option<String> = None;
            if let Some((rowid, search_text, content_type, content_text)) = item_data {
                sync_fts_delete(&tx, rowid, &search_text)?;
                if content_type == "image" {
                    image_path = Some(content_text);
                }
            }

            tx.commit().map_err(Self::map_db_err)?;

            Ok(DeleteResult {
                deleted: affected > 0,
                image_path,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn clear(&self, keep_favorites: bool) -> Result<CleanupResult, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn.transaction().map_err(Self::map_db_err)?;

            let image_paths = if keep_favorites {
                collect_image_paths_with_sql(
                    &tx,
                    "SELECT content_text FROM clipboard_items \
                     WHERE content_type = 'image' AND is_favorite = 0",
                    &[],
                )?
            } else {
                collect_image_paths_with_sql(
                    &tx,
                    "SELECT content_text FROM clipboard_items WHERE content_type = 'image'",
                    &[],
                )?
            };

            let sql = if keep_favorites {
                "DELETE FROM clipboard_items WHERE is_favorite = 0"
            } else {
                "DELETE FROM clipboard_items"
            };
            let affected = tx.execute(sql, []).map_err(Self::map_db_err)?;

            fts_rebuild(&tx)?;
            tx.commit().map_err(Self::map_db_err)?;

            Ok(CleanupResult {
                deleted_count: affected as u64,
                image_paths,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn enforce_max_count(&self, max_count: u32) -> Result<CleanupResult, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn.transaction().map_err(Self::map_db_err)?;

            let image_paths: Vec<String> = {
                let mut stmt = tx
                    .prepare(
                        "SELECT content_text FROM clipboard_items
                         WHERE content_type = 'image' AND is_favorite = 0
                           AND id IN (
                             SELECT id FROM clipboard_items
                             WHERE is_favorite = 0
                             ORDER BY last_copied_at ASC
                             LIMIT MAX(0,
                               (SELECT COUNT(*) FROM clipboard_items WHERE is_favorite = 0) - ?1
                             )
                           )",
                    )
                    .map_err(Self::map_db_err)?;
                let rows = stmt
                    .query_map(params![max_count], |row| row.get(0))
                    .map_err(Self::map_db_err)?;
                rows.filter_map(|r| r.ok()).collect()
            };

            let affected = tx
                .execute(
                    "DELETE FROM clipboard_items
                     WHERE is_favorite = 0
                       AND id IN (
                         SELECT id FROM clipboard_items
                         WHERE is_favorite = 0
                         ORDER BY last_copied_at ASC
                         LIMIT MAX(0,
                           (SELECT COUNT(*) FROM clipboard_items WHERE is_favorite = 0) - ?1
                         )
                       )",
                    params![max_count],
                )
                .map_err(Self::map_db_err)?;

            if affected > 0 {
                fts_rebuild(&tx)?;
            }

            tx.commit().map_err(Self::map_db_err)?;

            Ok(CleanupResult {
                deleted_count: affected as u64,
                image_paths,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn enforce_retention_days(
        &self,
        retention_days: i64,
    ) -> Result<CleanupResult, RepositoryError> {
        if retention_days <= 0 {
            return Ok(CleanupResult::default());
        }
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let cutoff = (Utc::now() - chrono::Duration::days(retention_days)).to_rfc3339();
            let tx = conn.transaction().map_err(Self::map_db_err)?;

            let image_paths: Vec<String> = {
                let mut stmt = tx
                    .prepare(
                        "SELECT content_text FROM clipboard_items
                         WHERE content_type = 'image' AND is_favorite = 0
                           AND last_copied_at < ?1",
                    )
                    .map_err(Self::map_db_err)?;
                let rows = stmt
                    .query_map(params![cutoff], |row| row.get(0))
                    .map_err(Self::map_db_err)?;
                rows.filter_map(|r| r.ok()).collect()
            };

            let affected = tx
                .execute(
                    "DELETE FROM clipboard_items
                     WHERE is_favorite = 0 AND last_copied_at < ?1",
                    params![cutoff],
                )
                .map_err(Self::map_db_err)?;

            if affected > 0 {
                fts_rebuild(&tx)?;
            }

            tx.commit().map_err(Self::map_db_err)?;

            Ok(CleanupResult {
                deleted_count: affected as u64,
                image_paths,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}
