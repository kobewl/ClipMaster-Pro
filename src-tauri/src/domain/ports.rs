//! Domain 定义的端口（接口）。Infrastructure 负责实现。

use crate::domain::error::{ClipboardSourceError, RepositoryError};
use crate::domain::model::{ClipGroup, ClipboardItem, ClipboardItemId, ContentType, NewClipboardItem};
use crate::domain::settings::AppSettings;
use async_trait::async_trait;

// ---------------------------------------------------------------------------
//  查询 / 结果结构
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    /// 按分组过滤。None = 全部条目。
    pub group_id: Option<String>,
    pub search_text: Option<String>,
    /// 按内容类型过滤（如只看图片）。None = 全部类型。
    pub content_type: Option<ContentType>,
    /// 只返回 created_at 不早于该时刻的条目（RFC3339 字符串，与列的存储
    /// 格式一致，字典序即时间序）。None = 不限时间。
    pub since: Option<String>,
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
    pub image_path: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CleanupResult {
    pub deleted_count: u64,
    pub image_paths: Vec<String>,
}

// ---------------------------------------------------------------------------
//  ClipboardRepository
// ---------------------------------------------------------------------------

#[async_trait]
pub trait ClipboardRepository: Send + Sync {
    async fn insert_or_touch(
        &self,
        item: NewClipboardItem,
    ) -> Result<ClipboardItem, RepositoryError>;

    async fn search(&self, query: SearchQuery) -> Result<ListResult, RepositoryError>;

    async fn get_by_id(&self, id: ClipboardItemId) -> Result<ClipboardItem, RepositoryError>;

    /// 设置条目的分组。group_id=None 表示移出任何分组。
    async fn set_group(
        &self,
        id: ClipboardItemId,
        group_id: Option<String>,
    ) -> Result<(), RepositoryError>;

    async fn delete(&self, id: ClipboardItemId) -> Result<DeleteResult, RepositoryError>;

    /// keep_grouped = true → 仅删除未分组条目。
    async fn clear(&self, keep_grouped: bool) -> Result<CleanupResult, RepositoryError>;

    async fn enforce_max_count(&self, max_count: u32) -> Result<CleanupResult, RepositoryError>;

    async fn enforce_retention_days(
        &self,
        retention_days: i64,
    ) -> Result<CleanupResult, RepositoryError>;
}

// ---------------------------------------------------------------------------
//  GroupRepository
// ---------------------------------------------------------------------------

#[async_trait]
pub trait GroupRepository: Send + Sync {
    async fn list_groups(&self) -> Result<Vec<ClipGroup>, RepositoryError>;
    async fn create_group(
        &self,
        name: String,
        color: String,
    ) -> Result<ClipGroup, RepositoryError>;
    async fn update_group(
        &self,
        id: String,
        name: String,
        color: String,
    ) -> Result<ClipGroup, RepositoryError>;
    async fn delete_group(&self, id: String) -> Result<(), RepositoryError>;
}

// ---------------------------------------------------------------------------
//  SettingsStore
// ---------------------------------------------------------------------------

#[async_trait]
pub trait SettingsStore: Send + Sync {
    async fn load(&self) -> Result<AppSettings, RepositoryError>;
    async fn save(&self, settings: AppSettings) -> Result<(), RepositoryError>;
}

// ---------------------------------------------------------------------------
//  剪贴板事件 & 平台适配器
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ClipboardEvent {
    pub content_type: crate::domain::model::ContentType,
    pub content_text: String,
    pub source_app: Option<String>,
    pub source_url: Option<String>,
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
    /// 写入富文本。实现方应**同时**写入对应的纯文本兜底，
    /// 保证只认纯文本的目标应用也能粘贴出内容。
    fn write_html(&self, html: &str) -> Result<(), ClipboardSourceError>;
    /// 写入一组文件引用（Finder 语义）。`paths` 为绝对路径列表。
    fn write_files(&self, paths: &[String]) -> Result<(), ClipboardSourceError>;
}
