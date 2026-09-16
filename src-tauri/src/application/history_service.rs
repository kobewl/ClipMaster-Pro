//! 历史记录相关用例：捕获、列表/搜索、收藏、删除、清空、复制。
//! 参考架构文档第 3.3 节：Application 编排具体用例，不含平台细节。

use std::str::FromStr;
use std::sync::Arc;

use crate::domain::error::{AppError, DomainError, RepositoryError};
use crate::domain::model::{ClipboardItem, ClipboardItemId, ContentType, NewClipboardItem};
use crate::domain::normalize::{build_search_text, compute_fingerprint, compute_fingerprint_bytes};
use crate::domain::ports::{
    ClipboardEvent, ClipboardRepository, ClipboardWriter, ListResult, SearchQuery,
};
use crate::infrastructure::sqlite::settings_store::SqliteSettingsStore;

/// 单条文本内容的大小上限（FR-CAP-005）。
pub const MAX_CONTENT_BYTES: usize = 5 * 1024 * 1024; // 5 MB

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

    /// 统一捕获入口，根据 content_type 分派到文本或图片路径。
    pub async fn capture(&self, event: ClipboardEvent) -> Result<ClipboardItem, AppError> {
        match event.content_type {
            ContentType::Text => {
                self.capture_text(event.content_text, event.source_app).await
            }
            ContentType::Image => {
                self.capture_image(event.content_text, event.source_app).await
            }
        }
    }

    /// 捕获文本。
    pub async fn capture_text(
        &self,
        content: String,
        source_app: Option<String>,
    ) -> Result<ClipboardItem, AppError> {
        if content.is_empty() {
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

        self.run_cleanup_after_capture().await?;
        Ok(item)
    }

    /// 捕获图片。content_text 是 EventHandler 已保存的 PNG 文件路径。
    /// fingerprint 从文件名中提取（文件名就是图片内容的 SHA-256 前缀）。
    async fn capture_image(
        &self,
        image_path: String,
        source_app: Option<String>,
    ) -> Result<ClipboardItem, AppError> {
        if image_path.is_empty() {
            return Err(AppError::Domain(DomainError::EmptyContent));
        }

        // fingerprint 从文件内容重新计算以保证一致性
        let bytes = tokio::fs::read(&image_path)
            .await
            .map_err(|e| AppError::Domain(DomainError::InvalidContentType(e.to_string())))?;
        let fingerprint = compute_fingerprint_bytes("image", &bytes);

        let item = self
            .repository
            .insert_or_touch(NewClipboardItem {
                content_type: ContentType::Image,
                content_text: image_path,
                fingerprint,
                source_app,
            })
            .await?;

        self.run_cleanup_after_capture().await?;
        Ok(item)
    }

    /// 捕获后执行数量上限 + 按天保留清理。
    async fn run_cleanup_after_capture(&self) -> Result<(), AppError> {
        let settings = self.settings_store.load().await?;
        self.repository
            .enforce_max_count(settings.max_history)
            .await?;
        self.repository
            .enforce_retention_days(settings.retention_days)
            .await?;
        Ok(())
    }

    pub async fn list(&self, query: SearchQuery) -> Result<ListResult, AppError> {
        Ok(self.repository.search(query).await?)
    }

    pub async fn get(&self, id_str: &str) -> Result<ClipboardItem, AppError> {
        let id = parse_id(id_str)?;
        Ok(self.repository.get_by_id(id).await?)
    }

    pub fn build_search_query(
        favorites_only: bool,
        search: Option<String>,
        limit: u32,
        offset: u32,
    ) -> SearchQuery {
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

    /// 将历史项重新写入系统剪贴板（支持文本和图片）。
    pub async fn copy_to_clipboard(&self, id_str: &str) -> Result<(), AppError> {
        let id = parse_id(id_str)?;
        let item = self.repository.get_by_id(id).await?;

        match item.content_type {
            ContentType::Text => {
                self.writer
                    .write_text(&item.content_text)
                    .map_err(AppError::Clipboard)?;
            }
            ContentType::Image => {
                self.writer
                    .write_image(&item.content_text)
                    .map_err(AppError::Clipboard)?;
            }
        }

        // 刷新 last_copied_at
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
    ClipboardItemId::from_str(id_str)
        .map_err(|_| AppError::Repository(RepositoryError::NotFound(id_str.to_string())))
}
