//! Command 层 DTO。

use serde::{Deserialize, Serialize};

use crate::domain::model::{build_preview, ClipGroup, ClipboardItem};
use crate::domain::settings::AppSettings;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ClipboardItemDto {
    pub id: String,
    pub content_type: &'static str,
    pub content_text: String,
    pub preview: String,
    pub group_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_copied_at: String,
    pub source_app: Option<String>,
    pub source_url: Option<String>,
}

impl From<ClipboardItem> for ClipboardItemDto {
    fn from(item: ClipboardItem) -> Self {
        use crate::domain::model::ContentType;
        let preview = match item.content_type {
            ContentType::Text => build_preview(&item.content_text),
            ContentType::Image => "[图片]".to_string(),
        };
        ClipboardItemDto {
            id: item.id.to_string(),
            content_type: item.content_type.as_str(),
            content_text: item.content_text,
            preview,
            group_id: item.group_id,
            created_at: item.created_at.to_rfc3339(),
            updated_at: item.updated_at.to_rfc3339(),
            last_copied_at: item.last_copied_at.to_rfc3339(),
            source_app: item.source_app,
            source_url: item.source_url,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListQueryDto {
    pub group_id: Option<String>,
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

// ---------------------------------------------------------------------------
//  Group DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClipGroupDto {
    pub id: String,
    pub name: String,
    pub color: String,
    pub sort_order: i32,
    pub item_count: u64,
}

impl From<ClipGroup> for ClipGroupDto {
    fn from(g: ClipGroup) -> Self {
        ClipGroupDto {
            id: g.id,
            name: g.name,
            color: g.color,
            sort_order: g.sort_order,
            item_count: g.item_count,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateGroupDto {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateGroupDto {
    pub id: String,
    pub name: String,
    pub color: String,
}

// ---------------------------------------------------------------------------
//  Settings DTO
// ---------------------------------------------------------------------------

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
