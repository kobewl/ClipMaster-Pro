//! Domain 定义的端口（接口）。Infrastructure 负责实现，Domain 本身不依赖具体技术。
//! 参考架构文档第 5.2 节、第 6 节。

use crate::domain::error::{ClipboardSourceError, RepositoryError};
use crate::domain::model::{ClipboardItem, ClipboardItemId, NewClipboardItem};
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub favorites_only: bool,
    /// 已经过 `build_search_text` 规范化的关键词，可为空。
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
}

#[async_trait]
pub trait ClipboardRepository: Send + Sync {
    /// 插入新记录；若 fingerprint 已存在则更新时间并保留收藏状态（去重策略，见 D-002）。
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

    /// 清空历史。`keep_favorites = true` 时保留收藏项（FR-MGT-004）。
    async fn clear(&self, keep_favorites: bool) -> Result<u64, RepositoryError>;

    /// 按数量上限清理，永远不删除收藏项（FR-MGT-006）。
    async fn enforce_max_count(&self, max_count: u32) -> Result<u64, RepositoryError>;

    /// 按保留天数清理，永远不删除收藏项。`retention_days <= 0` 时不执行任何删除。
    async fn enforce_retention_days(&self, retention_days: i64) -> Result<u64, RepositoryError>;
}

/// 剪贴板事件，由平台适配器产生，交给应用层用例处理。
#[derive(Debug, Clone)]
pub struct ClipboardEvent {
    pub content_text: String,
    pub source_app: Option<String>,
}

/// 剪贴板来源适配器接口（架构文档 5.2 节）。
/// `0.01 Beta` 只有 macOS 实现；接口本身不含平台细节。
pub trait ClipboardSource: Send {
    fn start(
        &mut self,
        sender: tokio::sync::mpsc::Sender<ClipboardEvent>,
    ) -> Result<(), ClipboardSourceError>;
    fn stop(&mut self);
}

/// 剪贴板写入适配器接口，独立于读取，方便测试用例中用假实现替换。
pub trait ClipboardWriter: Send + Sync {
    fn write_text(&self, content: &str) -> Result<(), ClipboardSourceError>;
}
