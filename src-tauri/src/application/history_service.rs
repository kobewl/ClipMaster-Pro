//! 历史记录相关用例：捕获、列表/搜索、收藏、删除、清空、复制。
//! 参考架构文档第 3.3 节：Application 编排具体用例，不含平台细节。

use std::str::FromStr;
use std::sync::Arc;

use crate::domain::error::{AppError, DomainError, RepositoryError};
use crate::domain::model::{ClipboardItem, ClipboardItemId, ContentType, NewClipboardItem};
use crate::domain::normalize::{build_search_text, compute_fingerprint};
use crate::domain::ports::{ClipboardRepository, ClipboardWriter, ListResult, SearchQuery};
use crate::infrastructure::sqlite::settings_store::SqliteSettingsStore;

/// 单条文本内容的大小上限（FR-CAP-005）。超限时跳过并只记录原因，不含正文。
pub const MAX_CONTENT_BYTES: usize = 5 * 1024 * 1024; // 5 MB，Beta 阶段的保守预算。

pub struct HistoryService {
    repository: Arc<dyn ClipboardRepository>,
    writer: Arc<dyn ClipboardWriter>,
    settings_store: Arc<SqliteSettingsStore>,
}

impl HistoryService {
    pub fn new(
        repository: Arc<dyn ClipboardRepository>,
        writer: Arc<dyn ClipboardWriter>,
        settings_store: Arc<SqliteSettingsStore>,
    ) -> Self {
        Self {
            repository,
            writer,
            settings_store,
        }
    }

    /// 捕获一段来自系统剪贴板的文本，完成校验、去重与保存，并执行清理策略。
    /// 对应采集流水线（架构文档第 6 节）中 CapturePolicy + Deduplicator + Repository 部分。
    pub async fn capture_text(
        &self,
        content: String,
        source_app: Option<String>,
    ) -> Result<ClipboardItem, AppError> {
        if content.is_empty() {
            // FR-CAP：空文本不入库。不对空白字符做额外裁剪判断，
            // 保留“默认保留原文”的标准化原则（D-003）。
            return Err(AppError::Domain(DomainError::EmptyContent));
        }

        let byte_len = content.len();
        if byte_len > MAX_CONTENT_BYTES {
            tracing::warn!(
                content_type = "text",
                size_bytes = byte_len,
                limit_bytes = MAX_CONTENT_BYTES,
                "内容超过大小上限，跳过采集"
            );
            return Err(AppError::Domain(DomainError::ContentTooLarge {
                actual: byte_len,
                limit: MAX_CONTENT_BYTES,
            }));
        }

        let fingerprint = compute_fingerprint("text", &content);
        let item = self
            .repository
            .insert_or_touch(NewClipboardItem {
                content_type: ContentType::Text,
                content_text: content,
                fingerprint,
                source_app,
            })
            .await?;

        // 捕获后立即执行数量上限与按天保留清理，收藏项始终受保护（FR-MGT-006）。
        // 按天清理同时由 lifecycle::runtime 中的小时级定时任务兜底触发
        // （长时间没有新捕获时，仅靠这里无法生效），两者共用同一个幂等的
        // repository 方法，重复调用不会产生副作用。
        let settings = self.settings_store.load().await?;
        self.repository
            .enforce_max_count(settings.max_history)
            .await?;
        self.repository
            .enforce_retention_days(settings.retention_days)
            .await?;

        Ok(item)
    }

    pub async fn list(&self, query: SearchQuery) -> Result<ListResult, AppError> {
        Ok(self.repository.search(query).await?)
    }

    /// 按 id 读取单条记录，供 Command 层在变更后重新查询最新状态并 emit 事件。
    pub async fn get(&self, id_str: &str) -> Result<ClipboardItem, AppError> {
        let id = parse_id(id_str)?;
        Ok(self.repository.get_by_id(id).await?)
    }

    pub fn build_search_query(favorites_only: bool, search: Option<String>, limit: u32, offset: u32) -> SearchQuery {
        SearchQuery {
            favorites_only,
            search_text: search
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(|s| build_search_text(&s)),
            limit,
            offset,
        }
    }

    pub async fn set_favorite(&self, id_str: &str, favorite: bool) -> Result<(), AppError> {
        let id = parse_id(id_str)?;
        self.repository.set_favorite(id, favorite).await?;
        Ok(())
    }

    pub async fn delete(&self, id_str: &str) -> Result<(), AppError> {
        let id = parse_id(id_str)?;
        let result = self.repository.delete(id).await?;
        if !result.deleted {
            return Err(AppError::Repository(RepositoryError::NotFound(
                id_str.to_string(),
            )));
        }
        Ok(())
    }

    pub async fn clear(&self, keep_favorites: bool) -> Result<u64, AppError> {
        Ok(self.repository.clear(keep_favorites).await?)
    }

    /// 将历史项重新写入系统剪贴板（FR-HIS-005 / US-005）。
    pub async fn copy_to_clipboard(&self, id_str: &str) -> Result<(), AppError> {
        let id = parse_id(id_str)?;
        let item = self.repository.get_by_id(id).await?;
        self.writer
            .write_text(&item.content_text)
            .map_err(AppError::Clipboard)?;

        // 重新复制不改变收藏状态，但需要刷新 last_copied_at，走与采集相同的
        // insert_or_touch 路径以复用去重逻辑（US-002 验收标准：应用写回不产生重复项）。
        self.repository
            .insert_or_touch(NewClipboardItem {
                content_type: item.content_type,
                content_text: item.content_text,
                fingerprint: item.fingerprint,
                source_app: item.source_app,
            })
            .await?;
        Ok(())
    }

    pub async fn run_retention_cleanup(&self, retention_days: i64) -> Result<u64, AppError> {
        Ok(self
            .repository
            .enforce_retention_days(retention_days)
            .await?)
    }
}

fn parse_id(id_str: &str) -> Result<ClipboardItemId, AppError> {
    ClipboardItemId::from_str(id_str).map_err(|_| {
        AppError::Repository(RepositoryError::NotFound(id_str.to_string()))
    })
}
