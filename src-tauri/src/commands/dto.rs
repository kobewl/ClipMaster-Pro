//! Command 层 DTO。只负责与前端的数据形状转换，不承载业务规则
//! （架构文档第 3.2 节：Command 不写业务规则）。

use serde::{Deserialize, Serialize};

use crate::domain::model::{build_preview, ClipboardItem};
use crate::domain::settings::AppSettings;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ClipboardItemDto {
    pub id: String,
    pub content_type: &'static str,
    pub content_text: String,
    pub preview: String,
    pub is_favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_copied_at: String,
    pub source_app: Option<String>,
}

impl From<ClipboardItem> for ClipboardItemDto {
    fn from(item: ClipboardItem) -> Self {
        ClipboardItemDto {
            id: item.id.to_string(),
            content_type: item.content_type.as_str(),
            preview: build_preview(&item.content_text),
            content_text: item.content_text,
            is_favorite: item.is_favorite,
            created_at: item.created_at.to_rfc3339(),
            updated_at: item.updated_at.to_rfc3339(),
            last_copied_at: item.last_copied_at.to_rfc3339(),
            source_app: item.source_app,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListQueryDto {
    pub favorites_only: bool,
    pub search: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ListResultDto {
    pub items: Vec<ClipboardItemDto>,
    pub total: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct AppSettingsDto {
    pub max_history: u32,
    pub retention_days: i64,
    pub capture_enabled: bool,
    pub shortcut: String,
}

impl From<AppSettings> for AppSettingsDto {
    fn from(s: AppSettings) -> Self {
        AppSettingsDto {
            max_history: s.max_history,
            retention_days: s.retention_days,
            capture_enabled: s.capture_enabled,
            shortcut: s.shortcut,
        }
    }
}

impl From<AppSettingsDto> for AppSettings {
    fn from(s: AppSettingsDto) -> Self {
        AppSettings {
            max_history: s.max_history,
            retention_days: s.retention_days,
            capture_enabled: s.capture_enabled,
            shortcut: s.shortcut,
        }
    }
}
