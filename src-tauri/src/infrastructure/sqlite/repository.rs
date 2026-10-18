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
use crate::infrastructure::sqlite::lock_conn;

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
fn relaxed_where(
    filters: &SearchQuery,
    term_templates: &[(String, String)],
) -> (String, Vec<String>) {
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
        conditions.push(format!(
            "clipboard_items.last_copied_at >= ?{}",
            binds.len()
        ));
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
        // 排序与严格路径一致：按复制时间倒序
        // （内存并集后还会按同一口径重排一次，两条子查询各自有序是为了让
        // LIMIT cap 截出来的是「最该被看到的 cap 条」）。
        let sql = format!(
            "SELECT clipboard_items.* FROM {from_clause} {where_sql}
             ORDER BY clipboard_items.last_copied_at DESC
             LIMIT ?{}",
            binds.len() + 1
        );
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
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
            source_bundle_id: row.get("source_bundle_id")?,
            source_app_path: row.get("source_app_path")?,
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

fn collect_image_paths_with_sql(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
) -> Result<Vec<String>, RepositoryError> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params, |row| row.get(0))
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 按行淘汰：逐行精确删 FTS 索引项 + 删主表行，返回需要清理的图片文件路径。
///
/// 这是 enforce_max_count / enforce_retention_days 共用的唯一淘汰机制：
/// 每个被删行用删除**前**读出的 search_text 精确删除它的索引项
/// （与 delete / insert_or_touch 插入分支同一机制，索引与主表列始终同步更新；
/// touch 分支不改 search_text，也就不动索引），
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
        tx.execute(
            "DELETE FROM clipboard_items WHERE rowid = ?1",
            params![rowid],
        )
        .map_err(|e| RepositoryError::Database(e.to_string()))?;
    }
    Ok((deleted_count, image_paths))
}

#[async_trait]
impl ClipboardRepository for SqliteClipboardRepository {
    /// 按指纹去重：命中旧行只「touch」，未命中才插入。
    ///
    /// - touch 路径只更新时间与四个来源字段，**不重算 search_text、不碰 FTS**：
    ///   指纹相同 = 规范化后的内容相同，`searchable_text` 的结果也就相同，
    ///   重建索引是纯开销（大文本每次复制都要重新小写 / 去标签一遍）。
    ///   代价：如果将来 `searchable_text` 的算法升级，现存行不会在 touch 时被顺手
    ///   刷新，需要专门做一次批量重建（目前现存行与当前算法一致：HTML 去标签与
    ///   HTML 类型是同一个提交引入的）。
    /// - 两个分支都不再 `SELECT *` 读回整行，返回值由「入参 + 元数据列」拼出。
    ///   因此命中旧行时，返回值的 `content_text` 是**调用方这次传入的值**，不保证
    ///   等于库里那一行：
    ///   - 文本 / HTML / 文件列表：与库里原文只可能在 CRLF / NFC 规范化上不同；
    ///   - 图片：指纹是 PNG 字节的哈希，`content_text` 是 watcher 这次算出的
    ///     绝对路径（`<image_dir>/<指纹前 32 位>.png`），库里那一行存的是第一次
    ///     复制时的路径。文件名由指纹决定，通常两者相同；但旧版本导入的行、
    ///     或数据目录变过之后，库里的路径可能不同 —— 所以**不要**拿返回值的
    ///     路径去清理重复图片，也不要把它当作库里的整条记录推给前端。
    ///
    ///   目前没有调用方读这个字段（采集管线只取 id 发事件，`copy_to_clipboard`
    ///   丢弃返回值）；需要库内原文的调用方请再 `get_by_id`。
    async fn insert_or_touch(
        &self,
        item: NewClipboardItem,
    ) -> Result<ClipboardItem, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = lock_conn(&conn);
            let now = Utc::now().to_rfc3339();
            let tx = conn.transaction().map_err(Self::map_db_err)?;

            // 只取拼返回值要用的元数据列：search_text / content_text 可能是几 MB。
            let existing: Option<(String, String, Option<String>, String, Option<String>)> = tx
                .query_row(
                    "SELECT id, content_type, group_id, created_at, legacy_id \
                     FROM clipboard_items WHERE fingerprint = ?1",
                    params![item.fingerprint],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .optional()
                .map_err(Self::map_db_err)?;

            if let Some((existing_id, stored_type, group_id, created_at, legacy_id)) = existing {
                tx.execute(
                    "UPDATE clipboard_items
                     SET updated_at = ?1, last_copied_at = ?1,
                         source_app = ?2, source_url = ?3,
                         source_bundle_id = ?4, source_app_path = ?5
                     WHERE id = ?6",
                    // 四个来源字段描述的是同一次（最近一次）复制，必须一起覆盖：
                    // 只换名字不换 bundle，会把 B 应用的名字配上 A 应用的图标。
                    params![
                        now,
                        item.source_app,
                        item.source_url,
                        item.source_bundle_id,
                        item.source_app_path,
                        existing_id
                    ],
                )
                .map_err(Self::map_db_err)?;
                tx.commit().map_err(Self::map_db_err)?;

                let touched_at = parse_datetime(&now);
                // id / content_type 的解析退路与 `row_to_item` 相同。
                return Ok(ClipboardItem {
                    id: ClipboardItemId::from_str(&existing_id).unwrap_or_default(),
                    content_type: ContentType::from_str(&stored_type).unwrap_or(ContentType::Text),
                    content_text: item.content_text,
                    fingerprint: item.fingerprint,
                    group_id,
                    created_at: parse_datetime(&created_at),
                    updated_at: touched_at,
                    last_copied_at: touched_at,
                    source_app: item.source_app,
                    source_url: item.source_url,
                    source_bundle_id: item.source_bundle_id,
                    source_app_path: item.source_app_path,
                    legacy_id,
                });
            }

            // 搜索文本与「命中证据」的重算口径必须一致：统一走 `searchable_text`
            // （HTML 去标签 + 小写），应用层装配证据时用的是同一个函数。
            // 只有新行才需要算（touch 路径见函数注释）。
            let search_text = searchable_text(item.content_type, &item.content_text);
            let new_id = ClipboardItemId::new();
            tx.execute(
                "INSERT INTO clipboard_items
                    (id, content_type, content_text, fingerprint, search_text,
                     is_favorite, group_id, created_at, updated_at, last_copied_at,
                     source_app, source_url, source_bundle_id, source_app_path, legacy_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, ?6, ?6, ?6, ?7, ?8, ?9, ?10, NULL)",
                params![
                    new_id.to_string(),
                    item.content_type.as_str(),
                    item.content_text,
                    item.fingerprint,
                    search_text,
                    now,
                    item.source_app,
                    item.source_url,
                    item.source_bundle_id,
                    item.source_app_path,
                ],
            )
            .map_err(Self::map_db_err)?;

            let rowid = get_rowid(&tx, &new_id.to_string())?;
            sync_fts_insert(&tx, rowid, &search_text)?;
            tx.commit().map_err(Self::map_db_err)?;

            // 刚插入的行就是这些值，不必再 SELECT * 读回全文。
            let inserted_at = parse_datetime(&now);
            Ok(ClipboardItem {
                id: new_id,
                content_type: item.content_type,
                content_text: item.content_text,
                fingerprint: item.fingerprint,
                group_id: None,
                created_at: inserted_at,
                updated_at: inserted_at,
                last_copied_at: inserted_at,
                source_app: item.source_app,
                source_url: item.source_url,
                source_bundle_id: item.source_bundle_id,
                source_app_path: item.source_app_path,
                legacy_id: None,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn search(&self, query: SearchQuery) -> Result<ListResult, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);

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
                // 按最近一次复制时间筛选，与列表的排序/显示口径一致，也能走
                // idx_clipboard_items_copied_time。last_copied_at 与 created_at 同由
                // Utc::now().to_rfc3339() 写入，字典序即时间序（见 time_range_to_since）。
                conditions.push(format!("clipboard_items.last_copied_at >= ?{pidx}"));
                param_values.push(Box::new(since.clone()));
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

            let count_sql = format!("SELECT COUNT(*) FROM {from_clause} {where_sql}");
            let count_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();
            let total: i64 = conn
                .query_row(&count_sql, count_refs.as_slice(), |row| row.get(0))
                .map_err(Self::map_db_err)?;

            pidx += 1;
            let limit_idx = pidx;
            pidx += 1;
            let offset_idx = pidx;
            let list_sql = format!(
                "SELECT clipboard_items.* FROM {from_clause} {where_sql}
                 ORDER BY clipboard_items.last_copied_at DESC
                 LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
            );
            param_values.push(Box::new(query.limit));
            param_values.push(Box::new(query.offset));
            let list_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            let mut stmt = conn.prepare(&list_sql).map_err(Self::map_db_err)?;
            let mut rows = stmt.query(list_refs.as_slice()).map_err(Self::map_db_err)?;
            let mut items = Vec::new();
            while let Some(row) = rows.next().map_err(Self::map_db_err)? {
                items.push(Self::row_to_item(row).map_err(Self::map_db_err)?);
            }

            Ok(ListResult {
                items,
                total: total as u64,
                matched_terms: None,
                relaxed_dropped: None,
            })
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
                let escaped = term
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
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
            merged.extend(
                self.run_relaxed_query(RELAXED_FTS_FROM, &where_sql, binds, cap)
                    .await?,
            );
        }
        if !like_patterns.is_empty() {
            let templates: Vec<(String, String)> = like_patterns
                .into_iter()
                .map(|pattern| {
                    (
                        "clipboard_items.search_text LIKE {} ESCAPE '\\'".to_string(),
                        pattern,
                    )
                })
                .collect();
            let (where_sql, binds) = relaxed_where(filters, &templates);
            merged.extend(
                self.run_relaxed_query("clipboard_items", &where_sql, binds, cap)
                    .await?,
            );
        }

        // 同一条可能既中长词又中短词：按 id 去重（保留先到的，字段完全一致）。
        let mut seen = std::collections::HashSet::new();
        merged.retain(|item| seen.insert(item.id.to_string()));
        // 与严格路径同一套排序口径：按复制时间倒序（分组不置顶）。
        merged.sort_by_key(|a| std::cmp::Reverse(a.last_copied_at));
        merged.truncate(cap as usize);
        Ok(merged)
    }

    async fn get_by_id(&self, id: ClipboardItemId) -> Result<ClipboardItem, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
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
            let conn = lock_conn(&conn);
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
            let mut conn = lock_conn(&conn);
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

    async fn clear(&self, keep_grouped: bool) -> Result<CleanupResult, RepositoryError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = lock_conn(&conn);
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
            let mut conn = lock_conn(&conn);
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
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(Self::map_db_err)?
            };

            let (deleted_count, image_paths) = evict_rows(&tx, victims)?;
            tx.commit().map_err(Self::map_db_err)?;
            Ok(CleanupResult {
                deleted_count,
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
            let mut conn = lock_conn(&conn);
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
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(Self::map_db_err)?
            };

            let (deleted_count, image_paths) = evict_rows(&tx, victims)?;
            tx.commit().map_err(Self::map_db_err)?;
            Ok(CleanupResult {
                deleted_count,
                image_paths,
            })
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    /// 会话候选取数。与 `search` 的 `since` 同样按 `last_copied_at` 开窗：会话按
    /// 「最近一次复制」成组，同一条内容被再复制一次就该回到工作记忆的顶端。
    /// 排序与列表路径同一口径（`last_copied_at DESC`），rowid 兜平局时的稳定序。
    async fn list_since(
        &self,
        since: &str,
        limit: u32,
    ) -> Result<Vec<ClipboardItem>, RepositoryError> {
        let conn = self.conn.clone();
        let since = since.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
            // last_copied_at 是 RFC3339 UTC 文本（row_to_item 的解析口径），
            // 同一写入路径生成，字典序即时间序。
            let mut stmt = conn
                .prepare(
                    "SELECT clipboard_items.* FROM clipboard_items
                     WHERE clipboard_items.last_copied_at >= ?1
                     ORDER BY clipboard_items.last_copied_at DESC, clipboard_items.rowid DESC
                     LIMIT ?2",
                )
                .map_err(Self::map_db_err)?;
            let rows = stmt
                .query_map(params![since, limit], Self::row_to_item)
                .map_err(Self::map_db_err)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(Self::map_db_err)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    /// 按 id 批量取：一条 `IN` 查询而不是 N 次单查（用户一次可能选中 20 条）；
    /// 只返回仍然存在的那些，顺序按 `last_copied_at ASC`（成员顺序口径，
    /// 和 `session_build` 的排序同一套）。
    async fn get_many(&self, ids: &[String]) -> Result<Vec<ClipboardItem>, RepositoryError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.clone();
        let ids = ids.to_vec();
        tokio::task::spawn_blocking(move || {
            let conn = lock_conn(&conn);
            // 占位符按 id 个数动态生成（不加引号拼接字符串，参数照旧走绑定）。
            let placeholders = (1..=ids.len())
                .map(|index| format!("?{index}"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT clipboard_items.* FROM clipboard_items
                 WHERE clipboard_items.id IN ({placeholders})
                 ORDER BY clipboard_items.last_copied_at ASC, clipboard_items.rowid ASC"
            );
            let mut stmt = conn.prepare(&sql).map_err(Self::map_db_err)?;
            let params: Vec<&dyn rusqlite::types::ToSql> = ids
                .iter()
                .map(|id| id as &dyn rusqlite::types::ToSql)
                .collect();
            let rows = stmt
                .query_map(params.as_slice(), Self::row_to_item)
                .map_err(Self::map_db_err)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(Self::map_db_err)
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::normalize::compute_fingerprint;
    use chrono::Duration;

    fn repo_with_conn() -> (SqliteClipboardRepository, Arc<Mutex<Connection>>) {
        let mut conn = Connection::open_in_memory().expect("内存库");
        crate::infrastructure::sqlite::migrations::run_migrations(&mut conn).expect("跑迁移");
        let conn = Arc::new(Mutex::new(conn));
        (SqliteClipboardRepository::new(conn.clone()), conn)
    }

    fn text_item(text: &str) -> NewClipboardItem {
        NewClipboardItem {
            content_type: ContentType::Text,
            content_text: text.to_string(),
            fingerprint: compute_fingerprint("text", text),
            source_app: None,
            source_url: None,
            source_bundle_id: None,
            source_app_path: None,
        }
    }

    fn query_all() -> SearchQuery {
        SearchQuery {
            group_id: None,
            search_text: None,
            content_type: None,
            since: None,
            limit: 100,
            offset: 0,
        }
    }

    fn one_day_ago() -> String {
        (Utc::now() - Duration::days(1)).to_rfc3339()
    }

    /// `recopied`：3 天前第一次复制、今天又复制了一次（只回拨 created_at）；
    /// `stale`：3 天前复制后再没碰过（两列都回拨）。
    fn backdate(conn: &Arc<Mutex<Connection>>, recopied_id: &str, stale_id: &str) {
        let three_days_ago = (Utc::now() - Duration::days(3)).to_rfc3339();
        // 作用域块：锁在这里就释放，不跨后面的 `.await`（clippy::await_holding_lock）。
        {
            let guard = conn.lock().unwrap();
            guard
                .execute(
                    "UPDATE clipboard_items SET created_at = ?1 WHERE id = ?2",
                    params![three_days_ago, recopied_id],
                )
                .unwrap();
            guard
                .execute(
                    "UPDATE clipboard_items SET created_at = ?1, last_copied_at = ?1 WHERE id = ?2",
                    params![three_days_ago, stale_id],
                )
                .unwrap();
        }
    }

    /// 两条都带一个 ≥3 字的词（`copied`，走 FTS）和一个 2 字的词（`ok`，走 LIKE）。
    async fn seed() -> (SqliteClipboardRepository, String, String) {
        let (repo, conn) = repo_with_conn();
        let recopied = repo
            .insert_or_touch(text_item("ok copied again today"))
            .await
            .unwrap();
        let stale = repo
            .insert_or_touch(text_item("ok copied long ago"))
            .await
            .unwrap();
        let (recopied_id, stale_id) = (recopied.id.to_string(), stale.id.to_string());
        backdate(&conn, &recopied_id, &stale_id);
        (repo, recopied_id, stale_id)
    }

    fn ids(items: &[ClipboardItem]) -> Vec<String> {
        items.iter().map(|item| item.id.to_string()).collect()
    }

    #[tokio::test]
    async fn strict_search_since_filters_on_last_copied_at() {
        let (repo, recopied_id, _stale_id) = seed().await;

        for search_text in [None, Some("copied"), Some("ok")] {
            let result = repo
                .search(SearchQuery {
                    since: Some(one_day_ago()),
                    search_text: search_text.map(str::to_string),
                    ..query_all()
                })
                .await
                .unwrap();
            assert_eq!(result.total, 1, "search_text = {search_text:?}");
            assert_eq!(
                ids(&result.items),
                vec![recopied_id.clone()],
                "按 last_copied_at 开窗：今天又复制过的那条要在，3 天前的不在（{search_text:?}）"
            );
        }

        let unfiltered = repo.search(query_all()).await.unwrap();
        assert_eq!(unfiltered.total, 2, "不限时间时两条都在");
    }

    fn search_text_of(conn: &Arc<Mutex<Connection>>, id: &str) -> String {
        conn.lock()
            .unwrap()
            .query_row(
                "SELECT search_text FROM clipboard_items WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .unwrap()
    }

    /// touch 不再改写 search_text：先把它改成哨兵值，再复制同一内容，
    /// 哨兵值必须原样留着（改写过就会变回 `hello world`）。
    #[tokio::test]
    async fn touch_does_not_rewrite_search_text() {
        let (repo, conn) = repo_with_conn();
        let first = repo
            .insert_or_touch(text_item("hello world"))
            .await
            .unwrap();
        let id = first.id.to_string();
        // 作用域块：锁在这里就释放，不跨后面的 `.await`（clippy::await_holding_lock）。
        {
            conn.lock()
                .unwrap()
                .execute(
                    "UPDATE clipboard_items SET search_text = 'sentinel-xyz' WHERE id = ?1",
                    params![id],
                )
                .unwrap();
        }

        let touched = repo
            .insert_or_touch(text_item("hello world"))
            .await
            .unwrap();

        assert_eq!(touched.id, first.id, "同指纹命中旧行");
        assert_eq!(search_text_of(&conn, &id), "sentinel-xyz");
        assert!(touched.last_copied_at >= first.last_copied_at);
        let stored = repo.get_by_id(first.id).await.unwrap();
        assert!(
            stored.last_copied_at >= first.last_copied_at,
            "库里的复制时间也前移了"
        );
    }

    /// 插入一次 + touch 两次：FTS 里仍只有一条索引项，搜索照常命中。
    #[tokio::test]
    async fn touch_keeps_single_fts_entry() {
        let (repo, conn) = repo_with_conn();
        for _ in 0..3 {
            repo.insert_or_touch(text_item("hello world"))
                .await
                .unwrap();
        }

        let fts_hits: i64 = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM clipboard_items_fts \
                 WHERE clipboard_items_fts MATCH '\"hello\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_hits, 1);
        assert_eq!(repo.search(query_all()).await.unwrap().total, 1);

        let found = repo
            .search(SearchQuery {
                search_text: Some("hello".into()),
                ..query_all()
            })
            .await
            .unwrap();
        assert_eq!(found.total, 1, "FTS 搜索仍能命中");
    }

    /// touch 的返回值由「入参 + 元数据列」拼出：id / created_at / group_id 来自库，
    /// 来源字段来自这次入参。
    #[tokio::test]
    async fn touch_returns_metadata_without_reading_text() {
        let (repo, conn) = repo_with_conn();
        let first = repo
            .insert_or_touch(text_item("hello world"))
            .await
            .unwrap();
        let group_id = "22222222-2222-2222-2222-222222222222".to_string();
        {
            let now = Utc::now().to_rfc3339();
            conn.lock()
                .unwrap()
                .execute(
                    "INSERT INTO clip_groups (id, name, color, sort_order, created_at, updated_at)
                     VALUES (?1, '测试', '#FF0000', 0, ?2, ?2)",
                    params![group_id, now],
                )
                .unwrap();
        }
        repo.set_group(first.id, Some(group_id.clone()))
            .await
            .unwrap();

        let touched = repo
            .insert_or_touch(NewClipboardItem {
                source_app: Some("Safari".into()),
                source_url: Some("https://example.com".into()),
                source_bundle_id: Some("com.apple.Safari".into()),
                source_app_path: Some("/Applications/Safari.app".into()),
                ..text_item("hello world")
            })
            .await
            .unwrap();

        assert_eq!(touched.id, first.id);
        assert_eq!(touched.created_at, first.created_at);
        assert_eq!(touched.group_id.as_deref(), Some(group_id.as_str()));
        assert_eq!(touched.content_type, ContentType::Text);
        assert_eq!(touched.source_app.as_deref(), Some("Safari"));
        assert_eq!(touched.source_url.as_deref(), Some("https://example.com"));
        assert_eq!(
            touched.source_bundle_id.as_deref(),
            Some("com.apple.Safari")
        );
        assert_eq!(
            touched.source_app_path.as_deref(),
            Some("/Applications/Safari.app")
        );

        // 拼出来的返回值与库里读回的整行一致（content_text 未规范化差异时）。
        let stored = repo.get_by_id(first.id).await.unwrap();
        assert_eq!(stored.group_id, touched.group_id);
        assert_eq!(stored.created_at, touched.created_at);
        assert_eq!(stored.last_copied_at, touched.last_copied_at);
        assert_eq!(stored.source_app, touched.source_app);
        assert_eq!(stored.content_text, touched.content_text);
    }

    #[tokio::test]
    async fn relaxed_search_since_filters_on_last_copied_at() {
        let (repo, recopied_id, stale_id) = seed().await;
        let windowed = SearchQuery {
            since: Some(one_day_ago()),
            ..query_all()
        };

        // `copied` 走 FTS 子查询，`ok` 走 LIKE 子查询。
        for term in ["copied", "ok"] {
            let items = repo
                .search_relaxed(&windowed, &[term.to_string()], 50)
                .await
                .unwrap();
            assert_eq!(ids(&items), vec![recopied_id.clone()], "term = {term}");

            let all = repo
                .search_relaxed(&query_all(), &[term.to_string()], 50)
                .await
                .unwrap();
            let mut all_ids = ids(&all);
            all_ids.sort();
            let mut expected = vec![recopied_id.clone(), stale_id.clone()];
            expected.sort();
            assert_eq!(all_ids, expected, "since 为 None 时两条都返回（{term}）");
        }
    }
}
