//! 历史记录相关用例。

use std::str::FromStr;
use std::sync::Arc;

use crate::domain::error::{AppError, DomainError, RepositoryError};
use crate::domain::model::{ClipboardItem, ClipboardItemId, ContentType, NewClipboardItem};
use crate::domain::normalize::{build_search_text, compute_fingerprint, compute_fingerprint_bytes};
use crate::domain::ports::{
    ClipboardEvent, ClipboardRepository, ClipboardWriter, ListResult, SearchQuery, SettingsStore,
};

pub const MAX_CONTENT_BYTES: usize = 5 * 1024 * 1024;

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

    async fn run_cleanup_after_capture(&self) -> Result<(), AppError> {
        let settings = self.settings_store.load().await?;
        let max_r = self.repository.enforce_max_count(settings.max_history).await?;
        spawn_image_cleanup(max_r.image_paths);
        let ret_r = self.repository.enforce_retention_days(settings.retention_days).await?;
        spawn_image_cleanup(ret_r.image_paths);
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
        group_id: Option<String>,
        search: Option<String>,
        limit: u32,
        offset: u32,
    ) -> SearchQuery {
        SearchQuery {
            group_id,
            search_text: search
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(|s| build_search_text(&s)),
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

    pub async fn run_retention_cleanup(&self, retention_days: i64) -> Result<u64, AppError> {
        let result = self.repository.enforce_retention_days(retention_days).await?;
        spawn_image_cleanup(result.image_paths);
        Ok(result.deleted_count)
    }
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
