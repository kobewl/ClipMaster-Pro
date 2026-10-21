//! Command 层 DTO。

use serde::{Deserialize, Serialize};

use crate::domain::model::{build_preview, ClipGroup, ClipboardItem};
use crate::domain::normalize::strip_html_tags;
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
    /// 来源应用的 bundle id；前端据此请求真实图标（旧记录为 null）。
    pub source_bundle_id: Option<String>,
    /// 来源应用 `.app` 的绝对路径（仅本机 webview 使用，旧记录为 null）。
    pub source_app_path: Option<String>,
    /// 这条命中了查询里的哪些词（词序同查询词序）。`None` = 本次查询为空，
    /// 序列化时整字段省略 —— 空 query 的响应因此与加字段之前逐字节一致。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_terms: Option<Vec<String>>,
}

impl From<ClipboardItem> for ClipboardItemDto {
    fn from(item: ClipboardItem) -> Self {
        // 列表行的两行正文（规则见 `item_preview`）—— 完整内容点开「查看全部」都能看到。
        let preview = item_preview(&item);
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
            source_bundle_id: item.source_bundle_id,
            source_app_path: item.source_app_path,
            matched_terms: None,
        }
    }
}

/// `clipboard://captured` / `clipboard://updated` 事件的 payload：只告诉前端
/// 「哪条变了」，前端收到后自己回查列表。
///
/// 不再推整条 `ClipboardItemDto`：那会带上 `content_text` 全文（最大 5MB），
/// 而唯一的消费者（前端历史列表）根本不读 payload，只拿它当刷新信号 ——
/// 每次复制都把全文序列化一遍、过一次 IPC，是纯开销。
#[derive(Debug, Clone, Serialize)]
pub struct ClipboardItemIdPayload {
    pub id: String,
}

/// 预览规则（列表行的两行正文）。
///
/// 图片 `[图片]`、HTML 去标签后折叠空白、文件只留文件名；规则只有这一份，
/// 改一处（比如换截断长度）不会出现第二份悄悄分叉的版本。
pub(crate) fn item_preview(item: &ClipboardItem) -> String {
    use crate::domain::model::ContentType;
    match item.content_type {
        ContentType::Text => build_preview(&item.content_text),
        ContentType::Image => "[图片]".to_string(),
        ContentType::Html => {
            let plain: String = strip_html_tags(&item.content_text)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            build_preview(&plain)
        }
        ContentType::Files => build_preview(&file_names_summary(&item.content_text)),
    }
}

/// 文件条目的列表预览：只要文件名，不要整串路径（路径太长会把两行占满）。
fn file_names_summary(files_text: &str) -> String {
    files_text
        .lines()
        .filter(|path| !path.trim().is_empty())
        .map(|path| {
            std::path::Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string())
        })
        .collect::<Vec<_>>()
        .join("、")
}

/// 请求一个来源图标：复制时记下的应用名 / bundle id / `.app` 路径。
///
/// 三个字段都可缺省：旧记录只有应用名，后端按名字兜底。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SourceIconRequestDto {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub bundle_id: Option<String>,
    #[serde(default)]
    pub app_path: Option<String>,
}

impl From<SourceIconRequestDto> for crate::infrastructure::appicon::IconSource {
    fn from(dto: SourceIconRequestDto) -> Self {
        Self {
            name: dto.name,
            bundle_id: dto.bundle_id,
            app_path: dto.app_path,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListQueryDto {
    pub group_id: Option<String>,
    pub search: Option<String>,
    /// 按内容类型筛选："text" | "image" | "html" | "files"。None = 全部。
    pub content_type: Option<String>,
    /// 预设时间档："today" | "week" | "month"。None = 全部时间。
    pub time_range: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ListResultDto {
    pub items: Vec<ClipboardItemDto>,
    pub total: u64,
    /// 本次放宽召回里"没有任何保留条目命中"的词（前端横幅用）。`None` = 没走放宽。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relaxed_dropped: Option<Vec<String>>,
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
    pub theme: String,
    /// 必填（不加 serde default）：漏传时直接报反序列化错误，
    /// 而不是悄悄把「不记录这些应用」名单清空。
    pub excluded_apps: Vec<String>,
}

impl From<AppSettings> for AppSettingsDto {
    fn from(s: AppSettings) -> Self {
        AppSettingsDto {
            max_history: s.max_history,
            retention_days: s.retention_days,
            capture_enabled: s.capture_enabled,
            shortcut: s.shortcut,
            theme: s.theme,
            excluded_apps: s.excluded_apps,
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
            theme: s.theme,
            excluded_apps: s.excluded_apps,
        }
    }
}

/// 历史里出现过的来源应用（设置面板「从最近复制来源中添加」）。
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct SourceAppDto {
    pub name: Option<String>,
    pub bundle_id: Option<String>,
}

impl From<crate::domain::capture_policy::SourceApp> for SourceAppDto {
    fn from(app: crate::domain::capture_policy::SourceApp) -> Self {
        Self {
            name: app.name,
            bundle_id: app.bundle_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::{ClipboardItem, ClipboardItemId, ContentType};

    fn sample_item() -> ClipboardItemDto {
        ClipboardItemDto {
            id: "11111111-1111-1111-1111-111111111111".into(),
            content_type: "text",
            content_text: "示例内容".into(),
            preview: "示例内容".into(),
            group_id: None,
            created_at: "2026-09-22T00:00:00+00:00".into(),
            updated_at: "2026-09-22T00:00:00+00:00".into(),
            last_copied_at: "2026-09-22T00:00:00+00:00".into(),
            source_app: None,
            source_url: None,
            source_bundle_id: None,
            source_app_path: None,
            matched_terms: None,
        }
    }

    /// bundle 字段总是序列化（缺失时为 null），前端据此请求真实图标。
    #[test]
    fn clipboard_item_dto_exposes_bundle_fields() {
        let with = ClipboardItemDto::from(ClipboardItem {
            source_bundle_id: Some("com.microsoft.VSCode".into()),
            source_app_path: Some("/Applications/Visual Studio Code.app".into()),
            ..sample_domain_item("示例", "2026-09-22T10:00:00Z")
        });
        let value = serde_json::to_value(&with).expect("序列化");
        assert_eq!(value["source_bundle_id"], "com.microsoft.VSCode", "{value}");
        assert_eq!(
            value["source_app_path"], "/Applications/Visual Studio Code.app",
            "{value}"
        );

        let without = ClipboardItemDto::from(sample_domain_item("示例", "2026-09-22T10:00:00Z"));
        let value = serde_json::to_value(&without).expect("序列化");
        let object = value.as_object().expect("对象");
        assert!(
            object.get("source_bundle_id").is_some_and(|v| v.is_null()),
            "{value}"
        );
        assert!(
            object.get("source_app_path").is_some_and(|v| v.is_null()),
            "{value}"
        );
    }

    /// captured / updated 事件的形状钉死为 `{ "id": … }`：不带全文，也不多带字段。
    #[test]
    fn clipboard_item_id_payload_serializes_only_id() {
        let value =
            serde_json::to_value(ClipboardItemIdPayload { id: "x".into() }).expect("序列化");
        assert_eq!(value, serde_json::json!({ "id": "x" }));
    }

    /// 前端可能只给部分字段（旧记录只有应用名），缺的字段落成 None。
    #[test]
    fn source_icon_request_deserializes_partial_objects() {
        let full: SourceIconRequestDto = serde_json::from_str(
            r#"{"name":"Safari","bundle_id":"com.apple.Safari","app_path":"/Applications/Safari.app"}"#,
        )
        .expect("完整请求");
        assert_eq!(full.name.as_deref(), Some("Safari"));
        assert_eq!(full.bundle_id.as_deref(), Some("com.apple.Safari"));
        assert_eq!(full.app_path.as_deref(), Some("/Applications/Safari.app"));

        let partial: SourceIconRequestDto =
            serde_json::from_str(r#"{"name":"Old"}"#).expect("只有名字的请求");
        assert_eq!(partial.name.as_deref(), Some("Old"));
        assert!(partial.bundle_id.is_none());
        assert!(partial.app_path.is_none());
    }

    /// 空 query 的响应必须与加证据字段之前**逐字节一致**：
    /// 新增字段在 `None` 时整字段省略，老前端解析到的 JSON 形状不变。
    #[test]
    fn evidence_fields_are_absent_when_none() {
        let json = serde_json::to_string(&ListResultDto {
            items: vec![sample_item()],
            total: 1,
            relaxed_dropped: None,
        })
        .expect("序列化列表响应");

        assert!(
            !json.contains("relaxed_dropped"),
            "空 query 不得出现放宽字段：{json}"
        );
        assert!(
            !json.contains("matched_terms"),
            "空 query 不得出现命中词字段：{json}"
        );
    }

    /// 有查询词时两个字段都如实出现（前端横幅与行内命中词直接读它们）。
    #[test]
    fn evidence_fields_appear_when_present() {
        let mut item = sample_item();
        item.matched_terms = Some(vec!["番茄牛腩".into()]);
        let json = serde_json::to_string(&ListResultDto {
            items: vec![item],
            total: 1,
            relaxed_dropped: Some(vec!["做法".into()]),
        })
        .expect("序列化列表响应");

        assert!(json.contains("\"matched_terms\":[\"番茄牛腩\"]"), "{json}");
        assert!(json.contains("\"relaxed_dropped\":[\"做法\"]"), "{json}");
    }

    /// 测试用的领域条目。DTO 只负责映射，这里的字段值只要够区分每一条。
    fn sample_domain_item(content_text: &str, copied_at: &str) -> ClipboardItem {
        let copied_at = rfc3339(copied_at);
        ClipboardItem {
            id: ClipboardItemId(
                uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("固定 uuid"),
            ),
            content_type: ContentType::Text,
            content_text: content_text.into(),
            fingerprint: "fp-1".into(),
            group_id: None,
            created_at: copied_at,
            updated_at: copied_at,
            last_copied_at: copied_at,
            source_app: Some("Safari".into()),
            source_url: None,
            source_bundle_id: None,
            source_app_path: None,
            legacy_id: None,
        }
    }

    fn rfc3339(value: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(value)
            .expect("测试时间字面量")
            .with_timezone(&chrono::Utc)
    }
}
