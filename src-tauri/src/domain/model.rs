//! 剪贴板核心领域模型。
//!
//! 参考文档：
//! - 02-架构与设计/01-总体技术架构.md 第 5.1 节
//! - 03-数据与迁移/01-数据模型存储与迁移方案.md 第 3.1 节

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 剪贴板条目稳定标识。使用 UUID v4，不依赖数据库自增 ID。
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

/// 内容类型。Text + Image。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    Image,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Image => "image",
        }
    }
}

impl std::str::FromStr for ContentType {
    type Err = crate::domain::error::DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(ContentType::Text),
            "image" => Ok(ContentType::Image),
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
    /// 文本类型：原始正文。图片类型：图片文件的绝对路径。
    pub content_text: String,
    pub fingerprint: String,
    pub is_favorite: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_copied_at: DateTime<Utc>,
    pub source_app: Option<String>,
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

/// 面向前端的截断预览长度上限。
pub const PREVIEW_MAX_CHARS: usize = 240;

pub fn build_preview(content: &str) -> String {
    if content.chars().count() <= PREVIEW_MAX_CHARS {
        return content.to_string();
    }
    let truncated: String = content.chars().take(PREVIEW_MAX_CHARS).collect();
    format!("{truncated}…")
}
