//! Domain 定义的端口（接口）。Infrastructure 负责实现，Domain 本身不依赖具体技术。

use crate::domain::error::{ClipboardSourceError, RepositoryError};
use crate::domain::model::{ClipboardItem, ClipboardItemId, NewClipboardItem};
use crate::domain::settings::AppSettings;
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub favorites_only: bool,
    pub search_text: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone)]
pub struct ListResult {
    pub items: Vec<ClipboardItem>,
    pub total: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DeleteResult {
    pub deleted: bool,
    /// 被删除条目的图片文件路径（仅 Image 类型），调用方需负责清理文件。
    pub image_path: Option<String>,
}

/// 批量清理操作的结果。包含被删除的图片路径列表，供 Application 层清理文件。
#[derive(Debug, Clone, Default)]
pub struct CleanupResult {
    pub deleted_count: u64,
    pub image_paths: Vec<String>,
}

#[async_trait]
pub trait ClipboardRepository: Send + Sync {
    async fn insert_or_touch(
        &self,
        item: NewClipboardItem,
    ) -> Result<ClipboardItem, RepositoryError>;

    async fn search(&self, query: SearchQuery) -> Result<ListResult, RepositoryError>;

    async fn get_by_id(&self, id: ClipboardItemId) -> Result<ClipboardItem, RepositoryError>;

    async fn set_favorite(
        &self,
        id: ClipboardItemId,
        favorite: bool,
    ) -> Result<(), RepositoryError>;

    async fn delete(&self, id: ClipboardItemId) -> Result<DeleteResult, RepositoryError>;

    async fn clear(&self, keep_favorites: bool) -> Result<CleanupResult, RepositoryError>;

    async fn enforce_max_count(&self, max_count: u32) -> Result<CleanupResult, RepositoryError>;

    async fn enforce_retention_days(
        &self,
        retention_days: i64,
    ) -> Result<CleanupResult, RepositoryError>;
}

/// 设置存储端口。Application 层通过此 trait 访问设置，不直接依赖 SQLite。
#[async_trait]
pub trait SettingsStore: Send + Sync {
    async fn load(&self) -> Result<AppSettings, RepositoryError>;
    async fn save(&self, settings: AppSettings) -> Result<(), RepositoryError>;
}

/// 剪贴板事件。
#[derive(Debug, Clone)]
pub struct ClipboardEvent {
    pub content_type: crate::domain::model::ContentType,
    pub content_text: String,
    pub source_app: Option<String>,
}

pub trait ClipboardSource: Send {
    fn start(
        &mut self,
        sender: tokio::sync::mpsc::Sender<ClipboardEvent>,
    ) -> Result<(), ClipboardSourceError>;
    fn stop(&mut self);
}

pub trait ClipboardWriter: Send + Sync {
    fn write_text(&self, content: &str) -> Result<(), ClipboardSourceError>;
    fn write_image(&self, image_path: &str) -> Result<(), ClipboardSourceError>;
}
