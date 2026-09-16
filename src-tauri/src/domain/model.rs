//! 剪贴板核心领域模型。
//!
//! 参考文档：
//! - 02-架构与设计/01-总体技术架构.md 第 5.1 节
//! - 03-数据与迁移/01-数据模型存储与迁移方案.md 第 3.1 节
//!
//! `0.01 Beta` 只承诺文本类型（ADR-004），但模型结构为未来内容类型预留扩展点。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 剪贴板条目稳定标识。使用 UUID v4，不依赖数据库自增 ID，
/// 避免像 v2 那样因为“delete+insert”式去重而导致 ID 漂移。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClipboardItemId(pub Uuid);

impl ClipboardItemId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ClipboardItemId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ClipboardItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ClipboardItemId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// 内容类型。`0.01 Beta` 只实现 Text；其余变体保留以对齐架构文档的扩展意图，
/// 目前未被任何采集路径产生（架构文档 5.1 节、范围文档 1.3 节“架构预留”）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
        }
    }
}

impl std::str::FromStr for ContentType {
    type Err = crate::domain::error::DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(ContentType::Text),
            other => Err(crate::domain::error::DomainError::InvalidContentType(
                other.to_string(),
            )),
        }
    }
}

/// 已持久化的剪贴板历史条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: ClipboardItemId,
    pub content_type: ContentType,
    /// 原始正文，保留用户输入的首尾空白（数据文档第 4 节 D-003 决策：默认保留原文）。
    pub content_text: String,
    /// 用于展示的截断预览，不在数据库中单独存储，读取时按需生成。
    pub fingerprint: String,
    pub is_favorite: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_copied_at: DateTime<Utc>,
    pub source_app: Option<String>,
    /// v2 迁移追踪字段，`0.01 Beta` 骨架阶段暂不写入，为 M5 导入器预留。
    pub legacy_id: Option<String>,
}

/// 新增条目时使用的输入结构，尚未分配 id/时间戳。
#[derive(Debug, Clone)]
pub struct NewClipboardItem {
    pub content_type: ContentType,
    pub content_text: String,
    pub fingerprint: String,
    pub source_app: Option<String>,
}

/// 面向前端的截断预览长度上限。完整内容仍可通过 `content_text` 获取（FR-HIS-006）。
pub const PREVIEW_MAX_CHARS: usize = 240;

pub fn build_preview(content: &str) -> String {
    if content.chars().count() <= PREVIEW_MAX_CHARS {
        return content.to_string();
    }
    let truncated: String = content.chars().take(PREVIEW_MAX_CHARS).collect();
    format!("{truncated}…")
}
