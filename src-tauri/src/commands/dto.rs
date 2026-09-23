//! Command 层 DTO。

use serde::{Deserialize, Serialize};

use crate::application::agent_service::AgentAction;
use crate::domain::model::{
    build_preview, AgentChatDetail, AgentChatMessage, AgentChatSummary, AgentSessionDetail,
    AgentSessionMember, AgentSessionSummary, ClipGroup, ClipboardItem, PlannerSuggestion,
    PlannerSuggestionSet, PlannerTarget,
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
            matched_terms: None,
        }
    }
}

/// 预览规则（列表行与 Planner 确认卡片**共用同一份**）。
///
/// 从上面的 `From` 里提出来不是为了复用几行代码，而是为了让两处只有一份规则：
/// 图片 `[图片]`、HTML 去标签后折叠空白、文件只留文件名。写成两份的话，
/// 将来改一处（比如换截断长度）另一边就会悄悄分叉。
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
    ///
    /// 认不出来的动作名原样显示；特例只有 Planner 的 `suggest_actions` ——
    /// 它**不是** `AgentAction` 的变体（那会让 Planner 能从 `run_agent_action`
    /// 被任意触发，白名单就不成立了），所以要在这里补一句人话，
    /// 否则用户在「使用记录」里看到一个英文串。
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
/// 三个数分别来自三个 store（会话 / 对话 / 审计）各自的事务：
/// 不做跨表事务，所以如实报出各清了多少。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ClearDerivedDataDto {
    pub sessions: u64,
    pub chats: u64,
    pub runs: u64,
}

// ---------------------------------------------------------------------------
//  Chat DTO（自然语言对话）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentChatSummaryDto {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: u64,
}

impl From<AgentChatSummary> for AgentChatSummaryDto {
    fn from(chat: AgentChatSummary) -> Self {
        Self {
            id: chat.id,
            title: chat.title,
            created_at: chat.created_at.to_rfc3339(),
            updated_at: chat.updated_at.to_rfc3339(),
            message_count: chat.message_count,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentChatMessageDto {
    pub role: String,
    pub content: String,
    pub created_at: String,
}

impl From<AgentChatMessage> for AgentChatMessageDto {
    fn from(message: AgentChatMessage) -> Self {
        Self {
            role: message.role,
            content: message.content,
            created_at: message.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentChatDetailDto {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<AgentChatMessageDto>,
}

impl From<AgentChatDetail> for AgentChatDetailDto {
    fn from(chat: AgentChatDetail) -> Self {
        Self {
            id: chat.id,
            title: chat.title,
            created_at: chat.created_at.to_rfc3339(),
            updated_at: chat.updated_at.to_rfc3339(),
            messages: chat.messages.into_iter().map(AgentChatMessageDto::from).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SendAgentChatDto {
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub message: String,
    #[serde(default)]
    pub item_ids: Vec<String>,
    #[serde(default)]
    pub search_hint: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

// ---------------------------------------------------------------------------
//  Planner 建议 DTO（Phase 1 第 5 步）
// ---------------------------------------------------------------------------

/// 在一条会话上求「下一步建议」。
///
/// `session_id` 是主语；`request_id` 由前端预置（理由同 `RunAgentActionDto`：
/// 界面要能按号取消、迟到响应要做守卫），并成为审计行的主键。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SuggestSessionActionsDto {
    pub session_id: String,
    #[serde(default)]
    pub request_id: Option<String>,
}

/// 一次建议的全部。**不落库**：返回值就是建议本体，关掉工作台即弃
/// （存建议等于存模型响应正文，与审计的隐私边界冲突）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PlannerSuggestionSetDto {
    pub suggestions: Vec<PlannerSuggestionDto>,
    /// 模型给了但不符合协议、被丢弃的行数（不静默吞掉，界面上如实告知）。
    pub dropped: u64,
}

/// 一条建议。
///
/// `action` 是 `&'static str` 字面量（照 `AgentSessionSummaryDto.source` 的写法）：
/// 前端直接拿它比较，序列化形状不受枚举改名影响。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PlannerSuggestionDto {
    /// 这次建议列表里的第几条（1-based，连续无空洞：被丢弃的行不占号）。
    pub index: u64,
    pub action: &'static str,
    /// `ai` 类的子动作 key（`summarize` …）。**不用 `skip_serializing_if`**：
    /// 前端要按 `null` 区分「不是 AI 动作」与「字段缺失」，显式下发 `null`
    /// （理由同 `AgentSessionMemberDto.reason`）。
    pub ai_action: Option<String>,
    pub targets: Vec<PlannerTargetDto>,
    /// 模型给的那句理由原文（说不清理由的行在后端就被丢了）。
    pub reason: String,
}

/// 一条建议的目标。
///
/// **刻意不用 [`ClipboardItemDto`]**：那个结构带 `content_text` 全文，而确认卡片
/// 要的是「哪一条 + 长什么样」—— 一条 5MB 的记录没必要为了显示 240 字的预览被
/// 整份搬运。取舍与 `AgentSessionMemberDto` 用全量 `ClipboardItemDto` 不同：
/// 那边详情页本来就要全文。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PlannerTargetDto {
    pub item_id: String,
    /// 会话成员**在库里的** 1-based 编号（界面上的编号就是它，不是 prompt 的 `[n]`）。
    pub position: i64,
    pub preview: String,
    pub source_app: Option<String>,
    /// 当前分组。让确认卡片能显示「当前分组」并识别「已经在这个分组里」的空转。
    pub group_id: Option<String>,
}

impl From<PlannerTarget> for PlannerTargetDto {
    fn from(target: PlannerTarget) -> Self {
        Self {
            // 预览与列表行共用同一份规则（`item_preview`），不在这里另算一套。
            preview: item_preview(&target.item),
            item_id: target.item.id.to_string(),
            position: target.position,
            source_app: target.item.source_app,
            group_id: target.item.group_id,
        }
    }
}

impl From<PlannerSuggestionSet> for PlannerSuggestionSetDto {
    fn from(set: PlannerSuggestionSet) -> Self {
        Self {
            // 建议顺序原样透传：服务层已按「模型给的顺序」编号（`index` 就是它）。
            suggestions: set
                .suggestions
                .into_iter()
                .map(PlannerSuggestionDto::from)
                .collect(),
            dropped: set.dropped,
        }
    }
}

impl From<PlannerSuggestion> for PlannerSuggestionDto {
    fn from(suggestion: PlannerSuggestion) -> Self {
        Self {
            index: suggestion.index,
            action: suggestion.kind.as_str(),
            ai_action: suggestion.ai_action,
            // 目标顺序原样透传：服务层已按 `position` 升序给出
            // （确认卡片上「第 N、M 条」的列举顺序就是它）。
            targets: suggestion
                .targets
                .into_iter()
                .map(PlannerTargetDto::from)
                .collect(),
            reason: suggestion.reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::{
        ClipboardItemId, ContentType, PlannerSuggestion, PlannerSuggestionKind,
        PlannerSuggestionSet, PlannerTarget, SessionSource,
    };

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
    /// 指定内容类型的领域条目：预览规则按类型分流（图片 `[图片]`、HTML 去标签），
    /// 所以每个类型都要能构造一条。
    fn sample_item_of(content_type: ContentType, content_text: &str) -> ClipboardItem {
        ClipboardItem {
            content_type,
            ..sample_domain_item(content_text, "2026-09-22T10:00:00Z")
        }
    }

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

    // -----------------------------------------------------------------------
    //  Planner 建议 DTO（Phase 1 第 5 步）
    // -----------------------------------------------------------------------

    fn sample_target(position: i64, content_type: ContentType, text: &str) -> PlannerTarget {
        PlannerTarget {
            item: sample_item_of(content_type, text),
            position,
        }
    }

    /// 建议 DTO 的前端契约：字段名 snake_case、`ai_action` **显式下发 `null`**
    /// （不用 `skip_serializing_if`，理由同 `AgentSessionMemberDto.reason`：
    /// 前端要按 `null` 区分「不是 AI 动作」与「字段缺失」）、`action` 是字面量
    /// （前端直接比较，不用枚举序列化）。
    #[test]
    fn planner_suggestions_serialize_the_frontend_contract() {
        let set = PlannerSuggestionSetDto::from(PlannerSuggestionSet {
            suggestions: vec![
                PlannerSuggestion {
                    index: 1,
                    kind: PlannerSuggestionKind::Copy,
                    // 非 ai 类：`ai_action` 是 `None` —— 必须显式下发 null。
                    ai_action: None,
                    targets: vec![sample_target(2, ContentType::Text, "示例内容")],
                    reason: "这条命令是刚才排查的结论，可以直接复制回终端".into(),
                },
                PlannerSuggestion {
                    index: 2,
                    kind: PlannerSuggestionKind::Ai,
                    ai_action: Some("summarize".into()),
                    targets: vec![
                        sample_target(1, ContentType::Text, "第一条"),
                        sample_target(3, ContentType::Text, "第三条"),
                    ],
                    reason: "把这次排查归纳成一段交接说明".into(),
                },
            ],
            dropped: 1,
        });

        let value = serde_json::to_value(&set).expect("序列化建议集");
        assert_eq!(value["dropped"], 1, "{value}");

        let copy = &value["suggestions"][0];
        assert_eq!(copy["index"], 1, "{value}");
        assert_eq!(copy["action"], "copy", "动作字面量直接下发：{value}");
        assert_eq!(
            copy["reason"],
            "这条命令是刚才排查的结论，可以直接复制回终端"
        );
        assert!(
            copy.as_object()
                .expect("建议是对象")
                .contains_key("ai_action"),
            "ai_action 字段必须在场（不能整字段省略）：{value}"
        );
        assert_eq!(
            copy["ai_action"],
            serde_json::Value::Null,
            "非 ai 类显式下发 null：{value}"
        );

        // 目标只给五个字段：`content_text` 全文不在这条链路上（确认卡片只要预览）。
        let target = &copy["targets"][0];
        assert_eq!(target["item_id"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(target["position"], 2, "编号用会话成员的 position：{value}");
        assert_eq!(target["preview"], "示例内容");
        assert_eq!(target["source_app"], "Safari");
        assert!(
            target
                .as_object()
                .expect("目标是对象")
                .contains_key("group_id"),
            "group_id 让确认卡片能显示「当前分组」并识别空转：{value}"
        );
        assert!(
            !target
                .as_object()
                .expect("目标是对象")
                .contains_key("content_text"),
            "刻意不用 ClipboardItemDto：一条 5MB 的记录不必为 240 字预览整份搬运：{value}"
        );

        // ai 类的子动作 key 原样透传（前端的窄化判据要拿它比对 AgentAction）。
        let ai = &value["suggestions"][1];
        assert_eq!(ai["action"], "ai", "{value}");
        assert_eq!(ai["ai_action"], "summarize", "{value}");
        assert_eq!(ai["targets"][1]["position"], 3, "{value}");
    }

    /// `item_preview` 提取后**行为逐字不变**：三条分流的输出与
    /// `ClipboardItemDto` 的 `preview` 逐字一致（两处共用同一份规则，
    /// 不在这里另算一套）。
    #[test]
    fn planner_target_preview_reuses_the_list_preview_rules() {
        // HTML 这条刻意**带换行、缩进与连续空格**：无空白差异的 HTML 上，
        // `split_whitespace().join(" ")` 是恒等变换 —— 那种 fixture 判别不出
        // 折叠有没有做（去掉折叠照样绿，假绿）。预览是给人看的，缩进和连续
        // 空格必须被折掉，所以用它当判别性输入。
        let html_with_whitespace = "<div>\n  Hello   <b>world</b>\n  </div>";
        let cases = [
            (ContentType::Text, "示例内容"),
            (ContentType::Image, ""),
            (ContentType::Html, html_with_whitespace),
            (
                ContentType::Files,
                "/Users/liang/Desktop/排查笔记.txt\n/Users/liang/Desktop/结论.md",
            ),
        ];

        for (content_type, text) in cases {
            let item = sample_item_of(content_type, text);
            let expected = ClipboardItemDto::from(item.clone()).preview;
            let target = PlannerTargetDto::from(PlannerTarget { item, position: 1 });
            assert_eq!(
                target.preview, expected,
                "{content_type:?} 的预览必须与 ClipboardItemDto 同一规则"
            );
        }

        // 三条分流各钉一个字面量：图片不留内容、HTML 去标签 + 折叠空白、
        // 文件只留文件名。
        assert_eq!(
            item_preview(&sample_item_of(ContentType::Image, "二进制痕迹")),
            "[图片]"
        );
        assert_eq!(
            item_preview(&sample_item_of(ContentType::Html, html_with_whitespace)),
            "Hello world",
            "HTML 预览要去标签**并折叠空白**：换行与连续空格不许漏进预览"
        );
        assert_eq!(
            item_preview(&sample_item_of(
                ContentType::Files,
                "/Users/liang/Desktop/排查笔记.txt\n/Users/liang/Desktop/结论.md"
            )),
            "排查笔记.txt、结论.md"
        );
    }
}
