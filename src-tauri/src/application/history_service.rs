//! 历史记录相关用例。

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;

use crate::application::query_parse;
use crate::application::rerank;
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
/// 而每条候选都要在内存里重算一遍命中词，所以这里给单次查询的成本钉一个上界。
/// 300 条足够覆盖真实剪贴板历史规模下的放宽场景。
const RELAX_RECALL_CAP: u32 = 300;

/// 重排窗口：一次查询进入重排的候选池上限（常量，边界写死在这里）。
///
/// 重排要看到**整个候选池**才可能给出全局稳定序，而分页在重排之后才切，所以
/// 取数时一次取到窗口上限。500 与前端单页上限（`MAX_PAGE_SIZE`）同量级 ——
/// 真实剪贴板历史里一次查询的命中很少超过这个数，超过的部分（第 501 条起）
/// **永远不可达**：这是本实现的已知边界，不是偶然的截断。放宽路径另有自己的
/// 召回 cap（[`RELAX_RECALL_CAP`]），两者互不影响。
///
/// **成本上界（可核对）**：窗口 500 × 单条内容上限 [`MAX_CONTENT_BYTES`]（5MB）
/// ⇒ 单次检索最多同时持有约 2.5GB 的原始文本，再加每条的 `search_text` 副本
/// （`ItemSignals` 持有它，与 `content_text` 同量级）。SQL 侧真正返回的条数还
/// 受历史库总量与筛选条件限制，这是**上限**而不是常态；两个常量任一上调都要
/// 重新核对这个乘积。
const RERANK_WINDOW_CAP: u32 = 500;

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

    /// 列表 / 检索的统一入口。判定顺序就是这段代码的骨架：浏览列表（空 query）→
    /// 严格查询（词间 AND）→ 放宽召回（严格 0 命中且词数 ≥2）。三档的边界条件与
    /// 各档 `total` 的口径见行内注释。
    ///
    /// **重排不变量**（本阶段新增，改动前请先读完这三条）：
    /// 1. **只有 query 非空时才重排**。空 query（浏览列表）完全走原路径：顺序与分页
    ///    仍由 SQL 的 `group_id / last_copied_at` 决定，逐条与重排前相同。
    /// 2. 重排先把候选池**一次取到窗口上限**（常量 `RERANK_WINDOW_CAP`，offset=0），
    ///    复用 `repository.search` 现成的 limit/offset 签名，**不动 SQL 排序**；
    ///    真正的分页在重排之后于内存里切 `[offset, offset+limit)`。所以同一 query 的
    ///    连续两页来自同一个稳定序，页间不会重复或丢失（窗口内）。
    /// 3. `total` 与重排无关，且在切页之前算好：严格路径 = 原 COUNT（同一 WHERE，
    ///    与 limit/offset 无关），放宽路径 = 过阈值后的候选数。两者口径都没变。
    pub async fn list(&self, query: SearchQuery) -> Result<ListResult, AppError> {
        let terms: Vec<String> = match query.search_text.as_deref() {
            Some(text) => query_parse::parse_query(text),
            None => Vec::new(),
        };
        // 空 query：完全走原来的 `search`，连证据字段都不装配（`matched_terms` /
        // `relaxed_dropped` 保持 `None`）—— 没有查询词可言，顺序与分页也必须零变化。
        if terms.is_empty() {
            return Ok(self.repository.search(query).await?);
        }

        // 同一次请求里只读一次「现在」，注入给纯函数：窗口里所有条目共用同一个
        // 时间基准，否则逐条打分之间的时间衰减会有微小但无意义的抖动。
        let now = Utc::now();
        // 单页上限一旦被调到窗口之上，`[offset, offset+limit)` 就会切到窗口外：
        // 请求 600 条时返回 500 条却没有任何报错，是静默的少返回。取数前先拦下。
        debug_assert!(
            query.limit <= RERANK_WINDOW_CAP,
            "单页上限 {} 超过重排窗口 {}：超出窗口的条目取不到，会静默少返回",
            query.limit,
            RERANK_WINDOW_CAP
        );
        // 取数窗口：**固定取窗口上限、offset=0**，与本次请求的页大小无关 ——
        // 重排是整池的属性，只取一页去重排等于每页各自最优（页序自相矛盾）；
        // 并且必须取到满窗口，否则 `[offset, offset+limit)` 切不出一页正常的
        // 结果（offset=2、窗口只有 2 条时第二页会空）。
        let window_query = SearchQuery {
            limit: RERANK_WINDOW_CAP,
            offset: 0,
            ..query.clone()
        };

        // 严格查询：词间 AND（多打一个词就 0 命中，这是正确行为）。只有 0 命中才
        // 考虑放宽；有命中就重排后返回，绝不掺入「只中一半」的条目。
        // 用解析后的词重写 search_text：同一份词集既是检索输入，也是装配证据的依据。
        let strict_query = SearchQuery {
            search_text: Some(terms.join(" ")),
            ..window_query
        };
        let strict = self.repository.search(strict_query).await?;
        if strict.total > 0 {
            let ranked = rerank::rerank_with_matched(strict.items, &terms, now);
            let (items, evidence) = slice_page(ranked, &query);
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

        // 放宽召回：OR 拉回候选，再按「命中词数 ≥ ⌈n/2⌉」过滤。`total` = 过滤后的
        // 保留条数（匹配总数，与 limit/offset 无关）。
        //
        // 过滤用的 `match_evidence_of` 与下面 rerank 内部的装配重复算了一遍，
        // 这是刻意的取舍：过滤必须先有命中数才能决定留不留，而重排的信号装配要
        // 吃整份文本；把中间结果传来传去只会让接口变复杂，候选数本身有 cap（300）。
        let candidates = self
            .repository
            .search_relaxed(&query, &terms, RELAX_RECALL_CAP)
            .await?;
        let threshold = query_parse::relax_threshold(terms.len());
        let mut kept: Vec<ClipboardItem> = Vec::new();
        // 每个查询词是否被至少一条保留条目命中 —— 横幅里「被筛除的词」就是没打上勾的那些。
        let mut term_hit = vec![false; terms.len()];
        for item in candidates {
            let matched = match_evidence_of(&item, &terms);
            if matched.len() < threshold {
                continue;
            }
            for (idx, term) in terms.iter().enumerate() {
                if matched.contains(term) {
                    term_hit[idx] = true;
                }
            }
            kept.push(item);
        }
        let total = kept.len() as u64;
        // 被筛掉词的口径：查询词里"没有任何一条保留条目命中"的词。
        // 只按保留条目的命中集合算，不宣称"这些词在库里不存在"——
        // 它们也可能只是没被放宽召回（cap）或没达到阈值。
        let dropped: Vec<String> = terms
            .iter()
            .zip(&term_hit)
            .filter(|(_, hit)| !**hit)
            .map(|(term, _)| term.clone())
            .collect();

        let ranked = rerank::rerank_with_matched(kept, &terms, now);
        let (items, evidence) = slice_page(ranked, &query);
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
    /// **不变量**：只要输入含非空白字符，`search_text` 就必须是 `Some` ——
    /// `None` 在下游等于「浏览列表」，会把用户的检索静默变成倒出整库。
    /// 纯符号查询（`😀`）因此也照常检索（可能 0 命中，但不会变成浏览）。
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

/// 从**已排好序**的候选池里切出本页，并把证据装成「条目 id → 命中词」。
///
/// `skip/take` 是纯内存操作，代价与窗口大小成正比，与内容体积无关；
/// 命中词的来源见 [`rerank::rerank_with_matched`] 的说明，这里只做搬运。
fn slice_page(
    ranked: Vec<(ClipboardItem, Vec<String>)>,
    query: &SearchQuery,
) -> (Vec<ClipboardItem>, HashMap<String, Vec<String>>) {
    let mut evidence = HashMap::new();
    let items = ranked
        .into_iter()
        .skip(query.offset as usize)
        .take(query.limit as usize)
        .map(|(item, matched)| {
            evidence.insert(item.id.to_string(), matched);
            item
        })
        .collect();
    (items, evidence)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 含非空白字符的查询串**永远**不能变成 `None`（浏览列表）。
    ///
    /// 这是静默降级的守门测试：曾经纯符号查询（`😀`）会被解析成空词集，
    /// 到下游变成「浏览整库」——用户以为在筛，实际看到的是全部历史。
    #[test]
    fn non_blank_query_never_falls_back_to_browsing() {
        for raw in ["😀", "→", "？？？", "C++", "C#", "★热点", "请问怎么做 redis", "的 了 吗"] {
            let query = HistoryService::build_search_query(
                None,
                Some(raw.to_string()),
                None,
                None,
                10,
                0,
            );
            assert!(
                query.search_text.is_some(),
                "查询 {raw:?} 不得退化成浏览列表（search_text 为 None）"
            );
        }

        // 反向：真正的空输入仍必须走浏览路径，否则列表页会去搜一个不存在的词。
        for raw in ["", "   ", "\n\t"] {
            let query = HistoryService::build_search_query(
                None,
                Some(raw.to_string()),
                None,
                None,
                10,
                0,
            );
            assert!(query.search_text.is_none(), "空白输入 {raw:?} 应按浏览列表处理");
        }
    }
}
