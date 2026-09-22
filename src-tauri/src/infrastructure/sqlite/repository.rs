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
use crate::domain::normalize::searchable_text;
use crate::domain::ports::{
    CleanupResult, ClipboardRepository, DeleteResult, ListResult, SearchQuery,
};

pub struct SqliteClipboardRepository {
    conn: Arc<Mutex<Connection>>,
}

/// 放宽召回里 FTS 子查询的取数子句：与严格路径相同的「主表 ⋈ FTS」口径。
const RELAXED_FTS_FROM: &str = "clipboard_items INNER JOIN clipboard_items_fts \
                                ON clipboard_items.rowid = clipboard_items_fts.rowid";

/// 拼放宽召回子查询的 WHERE：用户筛选条件（group / 类型 / 时间）之间是 AND，
/// 查询词之间是 OR（`term_templates` 多项时用 OR 连接，单项即原样）。
///
/// 返回 `(WHERE 子句, 绑定值)`。`?N` 序号按「先筛选条件、后查询词」分配，
/// 与绑定值入列顺序严格一致 —— 这里错位就会静默查出错误的数据。
fn relaxed_where(filters: &SearchQuery, term_templates: &[(String, String)]) -> (String, Vec<String>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref gid) = filters.group_id {
        binds.push(gid.clone());
        conditions.push(format!("clipboard_items.group_id = ?{}", binds.len()));
    }
    if let Some(ref content_type) = filters.content_type {
        binds.push(content_type.as_str().to_string());
        conditions.push(format!("clipboard_items.content_type = ?{}", binds.len()));
    }
    if let Some(ref since) = filters.since {
        binds.push(since.clone());
        conditions.push(format!("clipboard_items.created_at >= ?{}", binds.len()));
    }

    let term_expr = term_templates
        .iter()
        .map(|(template, value)| {
            binds.push(value.clone());
            template.replace("{}", &format!("?{}", binds.len()))
        })
        .collect::<Vec<_>>()
        .join(" OR ");
    conditions.push(format!("({term_expr})"));

    (format!("WHERE {}", conditions.join(" AND ")), binds)
}

impl SqliteClipboardRepository {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// 执行一条放宽召回子查询：`from_clause` + 已拼好的 WHERE，取 `cap` 条。
    ///
    /// 绑定值先以 `String` 收着、在这里才装箱成 `Box<dyn ToSql>`：
    /// `dyn ToSql` 不满足 Send，不能跨进 spawn_blocking 的闭包。
    async fn run_relaxed_query(
        &self,
        from_clause: &str,
        where_sql: &str,
        binds: Vec<String>,
        cap: u32,
    ) -> Result<Vec<ClipboardItem>, RepositoryError> {
        // 排序与严格路径一致：已分组优先、再按复制时间倒序
        // （内存并集后还会按同一口径重排一次，两条子查询各自有序是为了让
        // LIMIT cap 截出来的是「最该被看到的 cap 条」）。
        let sql = format!(
            "SELECT clipboard_items.* FROM {from_clause} {where_sql}
             ORDER BY (clipboard_items.group_id IS NULL) ASC,
                      clipboard_items.last_copied_at DESC
             LIMIT ?{}",
            binds.len() + 1
        );
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("sqlite mutex poisoned");
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = binds
                .into_iter()
                .map(|value| Box::new(value) as Box<dyn rusqlite::types::ToSql>)
                .collect();
            param_values.push(Box::new(cap));
            let refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&sql).map_err(Self::map_db_err)?;
            let mut rows = stmt.query(refs.as_slice()).map_err(Self::map_db_err)?;
            let mut items = Vec::new();
            while let Some(row) = rows.next().map_err(Self::map_db_err)? {
                items.push(Self::row_to_item(row).map_err(Self::map_db_err)?);
            }
            Ok(items)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    fn map_db_err(err: rusqlite::Error) -> RepositoryError {
        RepositoryError::Database(err.to_string())
    }

    /// 条目行的唯一映射口径。`pub(crate)` 而不是私有：会话详情要按同一口径把
    /// `clipboard_items` 的行映射成 `ClipboardItem`（`session_store::find` 的
    /// JOIN 取数），两份 row 映射迟早会漂移。
    pub(crate) fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardItem> {
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

/// RFC3339 文本 → `DateTime<Utc>`，解析失败退回当前时刻。
///
/// `pub(crate)`：会话的读模型（`session_store`）与条目行用同一套时间解析口径 ——
/// 与 `row_to_item` 同一个理由，两份解析的退路不同就会让同一条记录在
/// 两个接口里显示不同时间。
pub(crate) fn parse_datetime(value: &str) -> DateTime<Utc> {
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
            // 搜索文本与「命中证据」的重算口径必须一致：统一走 `searchable_text`
            // （HTML 去标签 + 小写），应用层装配证据时用的是同一个函数。
            let search_text = searchable_text(item.content_type, &item.content_text);
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

            Ok(ListResult { items, total: total as u64, matched_terms: None, relaxed_dropped: None })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    /// 见 trait 上的契约说明。这里的实现要点：
    /// - 用户筛选条件（group / 类型 / 时间）照常生效 —— 忽略它们会让「只看图片」
    ///   这类视图里冒出文本；
    /// - `filters.limit/offset/search_text` 不参与（分页由调用方在内存里做）。
    async fn search_relaxed(
        &self,
        filters: &SearchQuery,
        terms: &[String],
        cap: u32,
    ) -> Result<Vec<ClipboardItem>, RepositoryError> {
        // 长词（≥3 字）走 FTS、短词走 LIKE，**分两条查询再在内存里并集**。
        //
        // 为什么不合成一条 SQL：FTS5 的 MATCH 不能**直接**出现在 OR 表达式里
        // （`fts MATCH ?1 OR search_text LIKE ?2` 会被 SQLite 拒绝，报
        // "unable to use function MATCH in the requested context"），而放宽的语义
        // 恰恰是词间 OR。放进子查询（`rowid IN (SELECT rowid ... MATCH ?)`）也能绕过，
        // 但那样每个短词仍要走 LIKE，拆成两条反倒更直白。
        //
        // 查询词内部的 OR 用 FTS5 自己的查询语法表达（`"a" OR "b"`），
        // 这样长词侧完全走索引。

        // 长词：拼成 FTS5 的 OR 表达式。
        let fts_terms: Vec<String> = terms
            .iter()
            .filter(|term| term.chars().count() >= 3)
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect();
        // 短词（trigram 索引不到）：LIKE 兜底，同样是 OR。
        let like_patterns: Vec<String> = terms
            .iter()
            .filter(|term| !term.is_empty() && term.chars().count() < 3)
            .map(|term| {
                let escaped = term.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
                format!("%{escaped}%")
            })
            .collect();

        let mut merged: Vec<ClipboardItem> = Vec::new();
        if !fts_terms.is_empty() {
            let template = (
                "clipboard_items_fts MATCH {}".to_string(),
                fts_terms.join(" OR "),
            );
            let (where_sql, binds) = relaxed_where(filters, &[template]);
            merged
                .extend(self.run_relaxed_query(RELAXED_FTS_FROM, &where_sql, binds, cap).await?);
        }
        if !like_patterns.is_empty() {
            let templates: Vec<(String, String)> = like_patterns
                .into_iter()
                .map(|pattern| ("clipboard_items.search_text LIKE {} ESCAPE '\\'".to_string(), pattern))
                .collect();
            let (where_sql, binds) = relaxed_where(filters, &templates);
            merged.extend(self.run_relaxed_query("clipboard_items", &where_sql, binds, cap).await?);
        }

        // 同一条可能既中长词又中短词：按 id 去重（保留先到的，字段完全一致）。
        let mut seen = std::collections::HashSet::new();
        merged.retain(|item| seen.insert(item.id.to_string()));
        // 与严格路径同一套排序口径：已分组优先，再按复制时间倒序。
        merged.sort_by(|a, b| {
            a.group_id
                .is_none()
                .cmp(&b.group_id.is_none())
                .then_with(|| b.last_copied_at.cmp(&a.last_copied_at))
        });
        merged.truncate(cap as usize);
        Ok(merged)
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
