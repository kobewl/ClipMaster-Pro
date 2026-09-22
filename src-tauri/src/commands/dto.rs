//! Command 层 DTO。

use serde::{Deserialize, Serialize};

use crate::application::agent_service::AgentAction;
use crate::domain::model::{
    build_preview, AgentSessionDetail, AgentSessionMember, AgentSessionSummary, ClipGroup,
    ClipboardItem,
};
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
    /// 这条命中了查询里的哪些词（词序同查询词序）。`None` = 本次查询为空，
    /// 序列化时整字段省略 —— 空 query 的响应因此与加字段之前逐字节一致。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_terms: Option<Vec<String>>,
}

impl From<ClipboardItem> for ClipboardItemDto {
    fn from(item: ClipboardItem) -> Self {
        use crate::domain::model::ContentType;
        // 列表行的两行正文。HTML 用去标签后的纯文本（含空白折叠），
        // 文件用文件名清单 —— 完整内容点开「查看全部」都能看到。
        let preview = match item.content_type {
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
            matched_terms: None,
        }
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
}

impl From<AppSettings> for AppSettingsDto {
    fn from(s: AppSettings) -> Self {
        AppSettingsDto {
            max_history: s.max_history,
            retention_days: s.retention_days,
            capture_enabled: s.capture_enabled,
            shortcut: s.shortcut,
            theme: s.theme,
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
        }
    }
}

// ---------------------------------------------------------------------------
//  Agent DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunAgentActionDto {
    pub item_id: String,
    pub action: AgentAction,
    /// 前端预置的请求号（`crypto.randomUUID()`）。缺省时后端自己生成 ——
    /// 老版本前端与测试不必跟着改。有号才能定向取消，见 agent_service。
    #[serde(default)]
    pub request_id: Option<String>,
}

/// 多条内容一起跑一个动作（跨记录归纳）。
///
/// 与单条分成两个命令而不是一个「item_ids 数组」：
/// 单条那条 API 是 Phase 0 已经发出去的契约，改签名意味着前端、测试、
/// 文档三处都要跟着动，而它本身没有任何问题。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunAgentActionBatchDto {
    pub item_ids: Vec<String>,
    pub action: AgentAction,
    /// 同 `RunAgentActionDto::request_id`。
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SaveAgentKeyDto {
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SaveAgentEndpointDto {
    pub base_url: String,
    pub model: String,
}

/// 一条 AI 使用记录。**没有 prompt 正文和模型响应正文** —— 那些内容就在
/// 剪贴板历史里，审计再存一份等于把隐私面翻倍。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunDto {
    pub id: String,
    pub created_at: String,
    /// 机器可读的动作名，用于按动作聚合（评测与统计）。
    pub action: String,
    /// 界面直接显示的动作名。
    pub action_label: String,
    /// 请求发往的服务；None = 在本地就被拦下，没发出去。
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_item_ids: Vec<String>,
    pub input_chars: u64,
    /// `"ok"` 或 `"error"`。
    pub status: String,
    pub error_code: Option<String>,
    pub duration_ms: u64,
    pub output_chars: Option<u64>,
}

// ---------------------------------------------------------------------------
//  Session DTO（Flow 会话）
// ---------------------------------------------------------------------------

/// 列表用的会话摘要。
///
/// `source` 是 `&'static str` 字面量（`"user"` / `"agent"`）而不是枚举：
/// 前端直接拿它比较，序列化形状不受枚举改名影响；时间一律 `to_rfc3339()`
/// （照 `ClipboardItemDto` 的写法）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentSessionSummaryDto {
    pub id: String,
    pub source: &'static str,
    pub title: String,
    pub summary: String,
    pub created_at: String,
    pub updated_at: String,
    pub item_count: u64,
}

impl From<AgentSessionSummary> for AgentSessionSummaryDto {
    fn from(session: AgentSessionSummary) -> Self {
        Self {
            id: session.id,
            source: session.source.as_str(),
            title: session.title,
            summary: session.summary,
            created_at: session.created_at.to_rfc3339(),
            updated_at: session.updated_at.to_rfc3339(),
            item_count: session.item_count,
        }
    }
}

/// 会话详情：与摘要同样的字段面，外加成员列表。
///
/// 不给 `item_count`：详情的条数就是 `members.len()`，多一个字段就多一处
/// 可能与成员列表对不上的地方（摘要才需要它 —— 那边没有成员可数）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentSessionDetailDto {
    pub id: String,
    pub source: &'static str,
    pub title: String,
    pub summary: String,
    pub created_at: String,
    pub updated_at: String,
    pub members: Vec<AgentSessionMemberDto>,
}

impl From<AgentSessionDetail> for AgentSessionDetailDto {
    fn from(session: AgentSessionDetail) -> Self {
        Self {
            id: session.id,
            source: session.source.as_str(),
            title: session.title,
            summary: session.summary,
            created_at: session.created_at.to_rfc3339(),
            updated_at: session.updated_at.to_rfc3339(),
            // 成员顺序原样透传：服务层已按 `position` 升序给出
            // （关键判断 12），这里再排一次等于制造第二个排序真相源。
            members: session
                .members
                .into_iter()
                .map(AgentSessionMemberDto::from)
                .collect(),
        }
    }
}

/// 详情里的一条成员。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentSessionMemberDto {
    /// 条目本体，复用列表的条目 DTO（含 `preview`，前端不自己截断）。
    pub item: ClipboardItemDto,
    /// 会话内的编号，从 1 开始 —— 界面上的编号就是库里的编号。
    pub position: i64,
    /// `None` = 用户手动保存（他为什么把它们放在一起不需要机器解释）。
    ///
    /// **不用 `skip_serializing_if`**：前端要按 `null` 区分「用户手动保存」与
    /// 「agent 有理由」，显式下发 `null`。字段缺席与「没有理由」是两种读法，
    /// 整字段省略会让手动存的会话被当成「老版本响应」。
    pub reason: Option<String>,
}

impl From<AgentSessionMember> for AgentSessionMemberDto {
    fn from(member: AgentSessionMember) -> Self {
        Self {
            item: ClipboardItemDto::from(member.item),
            position: member.position,
            reason: member.reason,
        }
    }
}

/// 用户显式保存一次会话（多选若干条 → 「存为会话」）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateAgentSessionDto {
    /// 用户选中的条目 id。条数边界与「条目还在不在」由服务层判定并报错。
    pub item_ids: Vec<String>,
}

/// 一键清除 AI 派生数据的结果。
///
/// 两个数分别来自两个 store（会话 / 审计）各自的事务，见关键判断 11：
/// 不做跨表事务，所以如实报出两边各清了多少。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ClearDerivedDataDto {
    pub sessions: u64,
    pub runs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::{ClipboardItemId, ContentType, SessionSource};

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
            matched_terms: None,
        }
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

        assert!(!json.contains("relaxed_dropped"), "空 query 不得出现放宽字段：{json}");
        assert!(!json.contains("matched_terms"), "空 query 不得出现命中词字段：{json}");
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

    // -----------------------------------------------------------------------
    //  Session DTO（Flow 会话）
    // -----------------------------------------------------------------------

    /// 测试用的领域条目。会话 DTO 只负责映射，这里的字段值只要够区分每一条。
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
            legacy_id: None,
        }
    }

    fn rfc3339(value: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(value)
            .expect("测试时间字面量")
            .with_timezone(&chrono::Utc)
    }

    fn member_dto(position: i64, reason: Option<&str>, text: &str) -> AgentSessionMemberDto {
        AgentSessionMemberDto::from(AgentSessionMember {
            item: sample_domain_item(text, "2026-09-22T10:00:00Z"),
            position,
            reason: reason.map(str::to_string),
        })
    }

    /// `reason` 必须**显式**下发 `null`（字段在场，不用 `skip_serializing_if`）：
    /// 前端按 `null` 区分「用户手动保存」与「agent 有理由」——
    /// 字段缺席与「没有理由」是两种读法，混起来会让手动存的会话被误读成自动成组。
    #[test]
    fn session_member_reason_is_explicitly_null_when_none() {
        let dto = member_dto(1, None, "示例内容");
        let value = serde_json::to_value(&dto).expect("序列化会话成员");
        let object = value.as_object().expect("成员是对象");

        assert!(
            object.contains_key("reason"),
            "reason 字段必须在场（不能整字段省略）：{value}"
        );
        assert_eq!(value["reason"], serde_json::Value::Null, "{value}");
        assert!(
            serde_json::to_string(&dto)
                .expect("序列化会话成员")
                .contains("\"reason\":null"),
            "前端按字面量 null 判断，不能依赖字段缺席"
        );

        // 成员里的条目复用 `ClipboardItemDto::from` 的既有预览规则，
        // 不在这里另算一套（两套预览规则早晚会不一致）。
        assert_eq!(value["item"]["preview"], "示例内容", "{value}");
        assert_eq!(value["item"]["content_type"], "text", "{value}");
        assert_eq!(
            value["item"]["last_copied_at"], "2026-09-22T10:00:00+00:00",
            "时间一律 to_rfc3339()：{value}"
        );
        assert_eq!(value["position"], 1, "{value}");
    }

    /// agent 源的理由原文照传，且成员顺序**原样透传**服务层的 `position` 升序
    /// （Task 3 审查 M-2：DTO 层不重排、不做第二套排序假设 —— 界面上看到的
    /// 编号就是库里的编号，DTO 再排一次只会制造第二个真相源）。
    #[test]
    fn session_members_carry_reason_text_in_service_order() {
        let members: Vec<AgentSessionMemberDto> = vec![
            member_dto(1, Some("与前一条同源（Safari）"), "第一条"),
            member_dto(2, Some("与前一条共享关键词 redis"), "第二条"),
        ];
        let value = serde_json::to_value(&members).expect("序列化成员列表");
        let array = value.as_array().expect("成员列表是数组");

        assert_eq!(array[0]["item"]["content_text"], "第一条", "{value}");
        assert_eq!(array[1]["item"]["content_text"], "第二条", "{value}");
        assert_eq!(array[0]["position"], 1, "{value}");
        assert_eq!(array[1]["position"], 2, "{value}");
        assert_eq!(array[0]["reason"], "与前一条同源（Safari）", "{value}");
        assert_eq!(array[1]["reason"], "与前一条共享关键词 redis", "{value}");
    }

    /// 摘要与详情的字段面：`source` 下发前端直接比较的字面量（`"user"` 显示
    /// 「手动」小标），时间一律 `to_rfc3339()`，条数只由摘要给
    /// （详情的条数就是成员列表长度，不重复落字段）。
    #[test]
    fn session_summary_and_detail_serialize_the_frontend_contract() {
        let summary = AgentSessionSummaryDto::from(AgentSessionSummary {
            id: "sess-abc".into(),
            source: SessionSource::User,
            title: "3 条内容 · 10:00–10:12".into(),
            summary: "Safari 上的 3 条内容".into(),
            created_at: rfc3339("2026-09-22T10:00:00Z"),
            updated_at: rfc3339("2026-09-22T10:12:00Z"),
            item_count: 3,
        });
        let json = serde_json::to_string(&summary).expect("序列化会话摘要");
        assert!(json.contains("\"id\":\"sess-abc\""), "{json}");
        assert!(json.contains("\"source\":\"user\""), "{json}");
        assert!(json.contains("\"item_count\":3"), "{json}");
        assert!(
            json.contains("\"created_at\":\"2026-09-22T10:00:00+00:00\""),
            "{json}"
        );
        assert!(
            json.contains("\"updated_at\":\"2026-09-22T10:12:00+00:00\""),
            "{json}"
        );

        let detail = AgentSessionDetailDto::from(AgentSessionDetail {
            id: "sess-abc".into(),
            source: SessionSource::Agent,
            title: "3 条内容 · 10:00–10:12".into(),
            summary: "Safari 上的 3 条内容".into(),
            created_at: rfc3339("2026-09-22T10:00:00Z"),
            updated_at: rfc3339("2026-09-22T10:12:00Z"),
            members: vec![AgentSessionMember {
                item: sample_domain_item("第一条", "2026-09-22T10:00:00Z"),
                position: 1,
                reason: None,
            }],
        });
        let json = serde_json::to_string(&detail).expect("序列化会话详情");
        assert!(
            json.contains("\"source\":\"agent\""),
            "两种来源字面量都要钉住：{json}"
        );
        assert!(json.contains("\"members\":["), "{json}");
        assert!(
            !json.contains("\"item_count\""),
            "详情的条数由成员列表长度表达，不重复落字段：{json}"
        );
    }
}
