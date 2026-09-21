//! SQLite 实现的 ClipboardRepository。
//!
//! 分组系统：is_favorite 被 group_id 取代。group_id IS NOT NULL 的条目受清理保护。
//! 搜索：FTS5 trigram (migration 3)。事务：主表 + FTS 原子更新。

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::error::RepositoryError;
use crate::domain::model::{ClipboardItem, ClipboardItemId, ContentType, NewClipboardItem};
use crate::domain::normalize::{build_search_text, strip_html_tags};
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

        Ok(ClipboardItem {
            id: ClipboardItemId::from_str(&id_str).unwrap_or_default(),
            content_type: ContentType::from_str(&content_type_str).unwrap_or(ContentType::Text),
            content_text: row.get("content_text")?,
            fingerprint: row.get("fingerprint")?,
            group_id: row.get("group_id")?,
            created_at: parse_datetime(&created_at_str),
            updated_at: parse_datetime(&updated_at_str),
            last_copied_at: parse_datetime(&last_copied_at_str),
            source_app: row.get("source_app")?,
            source_url: row.get("source_url")?,
            legacy_id: row.get("legacy_id")?,
        })
    }
}

fn parse_datetime(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn sync_fts_insert(conn: &Connection, rowid: i64, search_text: &str) -> Result<(), RepositoryError> {
    conn.execute(
        "INSERT INTO clipboard_items_fts(rowid, search_text) VALUES (?1, ?2)",
        params![rowid, search_text],
    )
    .map_err(|e| RepositoryError::Database(format!("FTS insert failed: {e}")))?;
    Ok(())
}

fn sync_fts_delete(conn: &Connection, rowid: i64, old_search_text: &str) -> Result<(), RepositoryError> {
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

fn collect_image_paths_with_sql(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
) -> Result<Vec<String>, RepositoryError> {
    let mut stmt = conn.prepare(sql).map_err(|e| RepositoryError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params, |row| row.get(0))
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 按行淘汰：逐行精确删 FTS 索引项 + 删主表行，返回需要清理的图片文件路径。
///
/// 这是 enforce_max_count / enforce_retention_days 共用的唯一淘汰机制：
/// 每个被删行用删除**前**读出的 search_text 精确删除它的索引项
/// （与 delete / insert_or_touch 路径同一机制，索引与主表列始终同步更新），
/// 单行 O(log N)。
///
/// **不要**在这里用 `fts_rebuild` —— rebuild 是 O(N) 的全索引重建，
/// 在「历史已满、每次复制淘汰一条」的稳态下会退化成每次复制全量重建
/// （实测 2000 条 3.2ms/次、10000 条 16ms/次，随规模线性放大）。
/// 返回 `(删除的行数, 需要清理的图片文件路径)`。
fn evict_rows(
    tx: &rusqlite::Transaction<'_>,
    victims: Vec<(i64, String, String, String)>,
) -> Result<(u64, Vec<String>), RepositoryError> {
    let deleted_count = victims.len() as u64;
    let mut image_paths = Vec::new();
    for (rowid, search_text, content_type, content_text) in &victims {
        sync_fts_delete(tx, *rowid, search_text)?;
        if content_type.as_str() == "image" {
            image_paths.push(content_text.clone());
        }
        tx.execute("DELETE FROM clipboard_items WHERE rowid = ?1", params![rowid])
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
    }
    Ok((deleted_count, image_paths))
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
            // HTML 的搜索文本取去标签后的纯文本 —— 标签名不是用户想搜的内容；
            // 其余类型（文本 / 文件路径）直接用原文。
            let search_text = match item.content_type {
                ContentType::Html => build_search_text(&strip_html_tags(&item.content_text)),
                _ => build_search_text(&item.content_text),
            };
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
                     SET updated_at = ?1, last_copied_at = ?1,
                         source_app = ?2, source_url = ?3, search_text = ?4
                     WHERE id = ?5",
                    params![now, item.source_app, item.source_url, search_text, existing_id],
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
                     is_favorite, group_id, created_at, updated_at, last_copied_at,
                     source_app, source_url, legacy_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, ?6, ?6, ?6, ?7, ?8, NULL)",
                params![
                    new_id.to_string(),
                    item.content_type.as_str(),
                    item.content_text,
                    item.fingerprint,
                    search_text,
                    now,
                    item.source_app,
                    item.source_url,
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

            let mut fts_terms: Vec<String> = Vec::new();
            let mut like_patterns: Vec<String> = Vec::new();

            if let Some(ref text) = query.search_text {
                for word in text.split_whitespace() {
                    if word.is_empty() { continue; }
                    if word.chars().count() >= 3 {
                        let escaped = word.replace('"', "\"\"");
                        fts_terms.push(format!("\"{escaped}\""));
                    } else {
                        let escaped = word.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
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

            let mut conditions: Vec<String> = Vec::new();
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut pidx = 0usize;

            if let Some(ref gid) = query.group_id {
                pidx += 1;
                conditions.push(format!("clipboard_items.group_id = ?{pidx}"));
                param_values.push(Box::new(gid.clone()));
            }
            if let Some(ref content_type) = query.content_type {
                pidx += 1;
                conditions.push(format!("clipboard_items.content_type = ?{pidx}"));
                param_values.push(Box::new(content_type.as_str().to_string()));
            }
            if let Some(ref since) = query.since {
                pidx += 1;
                // created_at 是 RFC3339 UTC 文本，字典序即时间序（见 time_range_to_since）。
                conditions.push(format!("clipboard_items.created_at >= ?{pidx}"));
                param_values.push(Box::new(since.clone()));
            }
            if use_fts {
                pidx += 1;
                conditions.push(format!("clipboard_items_fts MATCH ?{pidx}"));
                param_values.push(Box::new(fts_expr));
            }
            for pattern in &like_patterns {
                pidx += 1;
                conditions.push(format!("clipboard_items.search_text LIKE ?{pidx} ESCAPE '\\'"));
                param_values.push(Box::new(pattern.clone()));
            }

            let where_sql = if conditions.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", conditions.join(" AND "))
            };

            let count_sql = format!("SELECT COUNT(*) FROM {from_clause} {where_sql}");
            let count_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
            let total: i64 = conn
                .query_row(&count_sql, count_refs.as_slice(), |row| row.get(0))
                .map_err(Self::map_db_err)?;

            pidx += 1;
            let limit_idx = pidx;
            pidx += 1;
            let offset_idx = pidx;
            let list_sql = format!(
                "SELECT clipboard_items.* FROM {from_clause} {where_sql}
                 ORDER BY (clipboard_items.group_id IS NULL) ASC,
                          clipboard_items.last_copied_at DESC
                 LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
            );
            param_values.push(Box::new(query.limit));
            param_values.push(Box::new(query.offset));
            let list_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

            let mut stmt = conn.prepare(&list_sql).map_err(Self::map_db_err)?;
            let mut rows = stmt.query(list_refs.as_slice()).map_err(Self::map_db_err)?;
            let mut items = Vec::new();
            while let Some(row) = rows.next().map_err(Self::map_db_err)? {
                items.push(Self::row_to_item(row).map_err(Self::map_db_err)?);
            }

            Ok(ListResult { items, total: total as u64 })
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
                rusqlite::Error::QueryReturnedNoRows => RepositoryError::NotFound(id.to_string()),
                other => Self::map_db_err(other),
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn set_group(
        &self,
        id: ClipboardItemId,
        group_id: Option<String>,
    ) -> Result<(), RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let now = Utc::now().to_rfc3339();
            let affected = conn
                .execute(
                    "UPDATE clipboard_items SET group_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![group_id, now, id.to_string()],
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
                .execute("DELETE FROM clipboard_items WHERE id = ?1", params![id.to_string()])
                .map_err(Self::map_db_err)?;

            let mut image_path: Option<String> = None;
            if let Some((rowid, search_text, content_type, content_text)) = item_data {
                sync_fts_delete(&tx, rowid, &search_text)?;
                if content_type == "image" {
                    image_path = Some(content_text);
                }
            }
            tx.commit().map_err(Self::map_db_err)?;
            Ok(DeleteResult { deleted: affected > 0, image_path })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn clear(&self, keep_grouped: bool) -> Result<CleanupResult, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn.transaction().map_err(Self::map_db_err)?;

            let image_paths = if keep_grouped {
                collect_image_paths_with_sql(
                    &tx,
                    "SELECT content_text FROM clipboard_items \
                     WHERE content_type = 'image' AND group_id IS NULL",
                    &[],
                )?
            } else {
                collect_image_paths_with_sql(
                    &tx,
                    "SELECT content_text FROM clipboard_items WHERE content_type = 'image'",
                    &[],
                )?
            };

            let sql = if keep_grouped {
                "DELETE FROM clipboard_items WHERE group_id IS NULL"
            } else {
                "DELETE FROM clipboard_items"
            };
            let affected = tx.execute(sql, []).map_err(Self::map_db_err)?;
            fts_rebuild(&tx)?;
            tx.commit().map_err(Self::map_db_err)?;
            Ok(CleanupResult { deleted_count: affected as u64, image_paths })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn enforce_max_count(&self, max_count: u32) -> Result<CleanupResult, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let tx = conn.transaction().map_err(Self::map_db_err)?;

            // 一趟收齐「待淘汰行」的全部所需列：按行删 FTS 要 rowid + search_text，
            // 图片文件清理要 content_text。
            // 此前这里是两条独立子查询（SELECT 图片路径 + DELETE）+ fts_rebuild，
            // 不仅每次复制都全量重建索引，两个子查询在 last_copied_at 打平时
            // 理论上还可能选中不同的行集 —— 单查询从根本上消除了这个分歧。
            let victims: Vec<(i64, String, String, String)> = {
                let mut stmt = tx
                    .prepare(
                        "SELECT rowid, search_text, content_type, content_text
                         FROM clipboard_items
                         WHERE group_id IS NULL
                         ORDER BY last_copied_at ASC
                         LIMIT MAX(0,
                           (SELECT COUNT(*) FROM clipboard_items WHERE group_id IS NULL) - ?1
                         )",
                    )
                    .map_err(Self::map_db_err)?;
                let rows = stmt
                    .query_map(params![max_count], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })
                    .map_err(Self::map_db_err)?;
                rows.collect::<Result<Vec<_>, _>>().map_err(Self::map_db_err)?
            };

            let (deleted_count, image_paths) = evict_rows(&tx, victims)?;
            tx.commit().map_err(Self::map_db_err)?;
            Ok(CleanupResult { deleted_count, image_paths })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn enforce_retention_days(&self, retention_days: i64) -> Result<CleanupResult, RepositoryError> {
        if retention_days <= 0 {
            return Ok(CleanupResult::default());
        }
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().expect("sqlite mutex poisoned");
            let cutoff = (Utc::now() - chrono::Duration::days(retention_days)).to_rfc3339();
            let tx = conn.transaction().map_err(Self::map_db_err)?;

            // 与 enforce_max_count 同一套淘汰机制：一趟收齐、按行删索引、按行删主表。
            let victims: Vec<(i64, String, String, String)> = {
                let mut stmt = tx
                    .prepare(
                        "SELECT rowid, search_text, content_type, content_text
                         FROM clipboard_items
                         WHERE group_id IS NULL AND last_copied_at < ?1",
                    )
                    .map_err(Self::map_db_err)?;
                let rows = stmt
                    .query_map(params![cutoff], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })
                    .map_err(Self::map_db_err)?;
                rows.collect::<Result<Vec<_>, _>>().map_err(Self::map_db_err)?
            };

            let (deleted_count, image_paths) = evict_rows(&tx, victims)?;
            tx.commit().map_err(Self::map_db_err)?;
            Ok(CleanupResult { deleted_count, image_paths })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}
