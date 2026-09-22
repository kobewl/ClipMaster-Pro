//! 历史记录相关用例。

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use crate::application::query_parse;
use crate::domain::error::{AppError, DomainError, RepositoryError};
use crate::domain::model::{ClipboardItem, ClipboardItemId, ContentType, NewClipboardItem};
use crate::domain::normalize::{
    compute_fingerprint, compute_fingerprint_bytes, searchable_text,
};
use crate::domain::ports::{
    ClipboardEvent, ClipboardRepository, ClipboardWriter, ListResult, SearchQuery, SettingsStore,
};

pub const MAX_CONTENT_BYTES: usize = 5 * 1024 * 1024;

/// 放宽召回的候选上限。
///
/// 放宽是「严格 0 命中的兜底」，不是主力检索：候选再多也不会都被用户看到，
/// 而每条候选都要在内存里重算一遍命中词。300 条足够覆盖真实历史的放宽场景，
/// 又给单次查询的成本钉了个上界（Task 2 的重排窗口 500 另算）。
const RELAX_RECALL_CAP: u32 = 300;

pub struct HistoryService {
    repository: Arc<dyn ClipboardRepository>,
    writer: Arc<dyn ClipboardWriter>,
    settings_store: Arc<dyn SettingsStore>,
}

impl HistoryService {
    pub fn new(
        repository: Arc<dyn ClipboardRepository>,
        writer: Arc<dyn ClipboardWriter>,
        settings_store: Arc<dyn SettingsStore>,
    ) -> Self {
        Self { repository, writer, settings_store }
    }

    pub async fn capture(&self, event: ClipboardEvent) -> Result<ClipboardItem, AppError> {
        match event.content_type {
            ContentType::Text => {
                self.capture_text(event.content_text, event.source_app, event.source_url).await
            }
            ContentType::Image => {
                self.capture_image(event.content_text, event.source_app, event.source_url).await
            }
            ContentType::Html => {
                self.capture_html(event.content_text, event.source_app, event.source_url).await
            }
            ContentType::Files => {
                self.capture_files(event.content_text, event.source_app, event.source_url).await
            }
        }
    }

    pub async fn capture_text(
        &self,
        content: String,
        source_app: Option<String>,
        source_url: Option<String>,
    ) -> Result<ClipboardItem, AppError> {
        if content.is_empty() {
            return Err(AppError::Domain(DomainError::EmptyContent));
        }
        let byte_len = content.len();
        if byte_len > MAX_CONTENT_BYTES {
            tracing::warn!(size_bytes = byte_len, "内容超过大小上限，跳过");
            return Err(AppError::Domain(DomainError::ContentTooLarge {
                actual: byte_len,
                limit: MAX_CONTENT_BYTES,
            }));
        }

        let fingerprint = compute_fingerprint("text", &content);
        let item = self.repository.insert_or_touch(NewClipboardItem {
            content_type: ContentType::Text,
            content_text: content,
            fingerprint,
            source_app,
            source_url,
        }).await?;

        self.run_cleanup_after_capture().await?;
        Ok(item)
    }

    async fn capture_image(
        &self,
        image_path: String,
        source_app: Option<String>,
        source_url: Option<String>,
    ) -> Result<ClipboardItem, AppError> {
        if image_path.is_empty() {
            return Err(AppError::Domain(DomainError::EmptyContent));
        }
        let bytes = tokio::fs::read(&image_path).await
            .map_err(|e| AppError::Domain(DomainError::InvalidContentType(e.to_string())))?;
        let fingerprint = compute_fingerprint_bytes("image", &bytes);

        let item = self.repository.insert_or_touch(NewClipboardItem {
            content_type: ContentType::Image,
            content_text: image_path,
            fingerprint,
            source_app,
            source_url,
        }).await?;

        self.run_cleanup_after_capture().await?;
        Ok(item)
    }

    /// 收一条富文本（HTML）。`content_text` 存**原始 HTML**；
    /// 搜索与列表预览用去标签后的纯文本（见 repository 的 search_text 构建）。
    async fn capture_html(
        &self,
        html: String,
        source_app: Option<String>,
        source_url: Option<String>,
    ) -> Result<ClipboardItem, AppError> {
        if html.trim().is_empty() {
            return Err(AppError::Domain(DomainError::EmptyContent));
        }
        let byte_len = html.len();
        if byte_len > MAX_CONTENT_BYTES {
            tracing::warn!(size_bytes = byte_len, "HTML 内容超过大小上限，跳过");
            return Err(AppError::Domain(DomainError::ContentTooLarge {
                actual: byte_len,
                limit: MAX_CONTENT_BYTES,
            }));
        }

        let fingerprint = compute_fingerprint("html", &html);
        let item = self.repository.insert_or_touch(NewClipboardItem {
            content_type: ContentType::Html,
            content_text: html,
            fingerprint,
            source_app,
            source_url,
        }).await?;

        self.run_cleanup_after_capture().await?;
        Ok(item)
    }

    /// 收一组文件引用。`content_text` 约定为绝对路径列表，用 `\n` 分隔。
    async fn capture_files(
        &self,
        files_text: String,
        source_app: Option<String>,
        source_url: Option<String>,
    ) -> Result<ClipboardItem, AppError> {
        let paths: Vec<String> = files_text
            .lines()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect();
        if paths.is_empty() {
            return Err(AppError::Domain(DomainError::EmptyContent));
        }

        let joined = paths.join("\n");
        let fingerprint = compute_fingerprint("files", &joined);
        let item = self.repository.insert_or_touch(NewClipboardItem {
            content_type: ContentType::Files,
            content_text: joined,
            fingerprint,
            source_app,
            source_url,
        }).await?;

        self.run_cleanup_after_capture().await?;
        Ok(item)
    }

    async fn run_cleanup_after_capture(&self) -> Result<(), AppError> {
        let settings = self.settings_store.load().await?;
        let max_r = self.repository.enforce_max_count(settings.max_history).await?;
        spawn_image_cleanup(max_r.image_paths);
        let ret_r = self.repository.enforce_retention_days(settings.retention_days).await?;
        spawn_image_cleanup(ret_r.image_paths);
        Ok(())
    }

    /// 列表 / 检索的统一入口。判定顺序就是这段代码的骨架：
    ///
    /// 1. **空 query（浏览列表）**：完全走原来的 `search`，
    ///    连证据字段都不装配（`matched_terms` / `relaxed_dropped` 保持 `None`）——
    ///    没有查询词可言，顺序与分页也必须零变化。
    /// 2. **严格查询**：词间 AND（多打一个词就 0 命中，这是正确行为）。
    ///    只有 0 命中才考虑放宽；有命中就照旧返回，绝不掺入"只中一半"的条目。
    /// 3. **放宽召回**：仅当严格 0 命中且词数 ≥2。OR 拉回候选后按
    ///    「命中词数 ≥ ⌈n/2⌉」过滤，再按 (命中数 desc, 时间 desc) 稳定排序、
    ///    内存切页；`total` = 过滤后的条数，`relaxed_dropped` 记录一条都没命中的词。
    pub async fn list(&self, query: SearchQuery) -> Result<ListResult, AppError> {
        let terms: Vec<String> = match query.search_text.as_deref() {
            Some(text) => query_parse::parse_query(text),
            None => Vec::new(),
        };
        if terms.is_empty() {
            return Ok(self.repository.search(query).await?);
        }

        // 严格路径：用解析后的词重写 search_text（同一份词集既是检索输入，
        // 也是后面装配证据的依据），其余筛选/分页参数原样透传。
        let strict_query = SearchQuery { search_text: Some(terms.join(" ")), ..query.clone() };
        let strict = self.repository.search(strict_query).await?;
        if strict.total > 0 {
            let mut evidence = HashMap::new();
            let items = strict
                .items
                .into_iter()
                .inspect(|item| {
                    evidence.insert(item.id.to_string(), match_evidence_of(item, &terms));
                })
                .collect();
            return Ok(ListResult {
                items,
                total: strict.total,
                matched_terms: Some(evidence),
                relaxed_dropped: None,
            });
        }
        // 单字/单词的查询没有"放宽"的余地：放宽到只命中 0 个词等于返回全库。
        if terms.len() < 2 {
            return Ok(ListResult {
                items: Vec::new(),
                total: 0,
                matched_terms: Some(HashMap::new()),
                relaxed_dropped: None,
            });
        }

        let candidates = self
            .repository
            .search_relaxed(&query, &terms, RELAX_RECALL_CAP)
            .await?;
        let threshold = query_parse::relax_threshold(terms.len());
        let mut kept: Vec<(Vec<String>, ClipboardItem)> = Vec::new();
        for item in candidates {
            let matched = match_evidence_of(&item, &terms);
            if matched.len() >= threshold {
                kept.push((matched, item));
            }
        }
        // 稳定序：命中多的在前；同命中数按复制时间倒序（重排在 Task 2，这里只保证确定）。
        kept.sort_by(|(matched_a, item_a), (matched_b, item_b)| {
            matched_b
                .len()
                .cmp(&matched_a.len())
                .then_with(|| item_b.last_copied_at.cmp(&item_a.last_copied_at))
        });

        let total = kept.len() as u64;
        // 被筛掉词的口径：查询词里"没有任何一条保留条目命中"的词。
        // 只按保留条目的命中集合算，不宣称"这些词在库里不存在"——
        // 它们也可能只是没被放宽召回（cap）或没达到阈值。
        let dropped: Vec<String> = terms
            .iter()
            .filter(|term| !kept.iter().any(|(matched, _)| matched.contains(term)))
            .cloned()
            .collect();

        let page: Vec<(Vec<String>, ClipboardItem)> = kept
            .into_iter()
            .skip(query.offset as usize)
            .take(query.limit as usize)
            .collect();
        let mut evidence = HashMap::new();
        let items = page
            .into_iter()
            .map(|(matched, item)| {
                evidence.insert(item.id.to_string(), matched);
                item
            })
            .collect();
        Ok(ListResult {
            items,
            total,
            matched_terms: Some(evidence),
            relaxed_dropped: Some(dropped),
        })
    }

    pub async fn get(&self, id_str: &str) -> Result<ClipboardItem, AppError> {
        let id = parse_id(id_str)?;
        Ok(self.repository.get_by_id(id).await?)
    }

    /// 把前端传进来的查询串归一成检索输入：走 [`query_parse::parse_query`]，
    /// 停用词与句末标点在**进入 SQL 之前**就被剥掉，词间用空格连接
    /// （repository 侧按空白重新分词，两边口径一致）。
    ///
    /// 唯一会解析成空串的输入是「全部由标点组成」（parse_query 的保底也只能
    /// 回到原始词，仍是空），这种输入按浏览列表处理 —— 它本来就不含任何检索意图。
    pub fn build_search_query(
        group_id: Option<String>,
        search: Option<String>,
        content_type: Option<ContentType>,
        since: Option<String>,
        limit: u32,
        offset: u32,
    ) -> SearchQuery {
        SearchQuery {
            group_id,
            content_type,
            since,
            search_text: search
                .map(|s| query_parse::parse_query(&s).join(" "))
                .filter(|s| !s.is_empty()),
            limit,
            offset,
        }
    }

    pub async fn set_group(&self, id_str: &str, group_id: Option<String>) -> Result<(), AppError> {
        let id = parse_id(id_str)?;
        self.repository.set_group(id, group_id).await?;
        Ok(())
    }

    pub async fn delete(&self, id_str: &str) -> Result<(), AppError> {
        let id = parse_id(id_str)?;
        let result = self.repository.delete(id).await?;
        if !result.deleted {
            return Err(AppError::Repository(RepositoryError::NotFound(id_str.to_string())));
        }
        if let Some(path) = result.image_path {
            spawn_image_cleanup(vec![path]);
        }
        Ok(())
    }

    pub async fn clear(&self, keep_grouped: bool) -> Result<u64, AppError> {
        let result = self.repository.clear(keep_grouped).await?;
        spawn_image_cleanup(result.image_paths);
        Ok(result.deleted_count)
    }

    pub async fn copy_to_clipboard(&self, id_str: &str) -> Result<(), AppError> {
        let id = parse_id(id_str)?;
        let item = self.repository.get_by_id(id).await?;
        match item.content_type {
            ContentType::Text => self.writer.write_text(&item.content_text).map_err(AppError::Clipboard)?,
            ContentType::Image => self.writer.write_image(&item.content_text).map_err(AppError::Clipboard)?,
            ContentType::Html => self.writer.write_html(&item.content_text).map_err(AppError::Clipboard)?,
            ContentType::Files => {
                let paths: Vec<String> = item
                    .content_text
                    .lines()
                    .filter(|path| !path.trim().is_empty())
                    .map(str::to_string)
                    .collect();
                self.writer.write_files(&paths).map_err(AppError::Clipboard)?;
            }
        }
        self.repository.insert_or_touch(NewClipboardItem {
            content_type: item.content_type,
            content_text: item.content_text,
            fingerprint: item.fingerprint,
            source_app: item.source_app,
            source_url: item.source_url,
        }).await?;
        Ok(())
    }

    /// 把任意文本写进剪贴板（多选合并复制用）。
    ///
    /// 与 [`Self::copy_to_clipboard`] 的关键差别：**不调用 `insert_or_touch`**。
    /// 合并出来的文本是临时产物，没有对应的来源应用和时间，塞进历史只会变成
    /// 一条查不到出处、也没法解释的记录。
    ///
    /// 那它会不会被采集管线当成「用户新复制的内容」收进去？不会 ——
    /// `ClipboardWriter` 写剪贴板前会记下这次写入的 fingerprint，
    /// 采集侧读到同样的内容时会消费掉这个标记并跳过（self-write guard）。
    pub async fn copy_text_to_clipboard(&self, text: &str) -> Result<(), AppError> {
        if text.is_empty() {
            return Err(AppError::Domain(DomainError::EmptyContent));
        }
        self.writer.write_text(text).map_err(AppError::Clipboard)?;
        Ok(())
    }

    pub async fn run_retention_cleanup(&self, retention_days: i64) -> Result<u64, AppError> {
        let result = self.repository.enforce_retention_days(retention_days).await?;
        spawn_image_cleanup(result.image_paths);
        Ok(result.deleted_count)
    }
}

/// 单条条目的命中证据：按 `search_text` 的同一口径（HTML 先去标签）逐词
/// `contains` 判断，返回命中的词（顺序跟随查询词序）。
///
/// **重要不变量**：HTML 判定用的纯文本由 [`searchable_text`] 生成，
/// 与写库时构建 `search_text` 是同一个函数 —— 换算法必须两处同时换，
/// 否则证据会与真正的检索结果对不上。
fn match_evidence_of(item: &ClipboardItem, terms: &[String]) -> Vec<String> {
    let text = searchable_text(item.content_type, &item.content_text);
    query_parse::matched_terms(&text, terms)
}

fn spawn_image_cleanup(paths: Vec<String>) {
    if paths.is_empty() { return; }
    tokio::spawn(async move {
        for path in &paths {
            if let Err(e) = tokio::fs::remove_file(path).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(path, error = %e, "清理孤儿图片文件失败");
                }
            }
        }
    });
}

fn parse_id(id_str: &str) -> Result<ClipboardItemId, AppError> {
    ClipboardItemId::from_str(id_str)
        .map_err(|_| AppError::Repository(RepositoryError::NotFound(id_str.to_string())))
}
