//! 剪贴板核心领域模型。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 剪贴板条目稳定标识。
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

/// 内容类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    Image,
    /// 富文本（HTML）。`content_text` 存原始 HTML；搜索与预览用去标签后的纯文本。
    Html,
    /// 一组文件引用（Finder 里 ⌘C 文件）。`content_text` 存各文件绝对路径，以 `\n` 分隔。
    Files,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Image => "image",
            ContentType::Html => "html",
            ContentType::Files => "files",
        }
    }
}

impl std::str::FromStr for ContentType {
    type Err = crate::domain::error::DomainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(ContentType::Text),
            "image" => Ok(ContentType::Image),
            "html" => Ok(ContentType::Html),
            "files" => Ok(ContentType::Files),
            other => Err(crate::domain::error::DomainError::InvalidContentType(
                other.to_string(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
//  分组
// ---------------------------------------------------------------------------

/// 剪贴板条目分组。替代原有的布尔收藏，支持多组 + 自定义颜色。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipGroup {
    pub id: String,
    pub name: String,
    /// CSS 颜色值，如 "#3B82F6"。
    pub color: String,
    pub sort_order: i32,
    /// 组内条目数量（由查询时 LEFT JOIN 计算得出）。
    pub item_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
//  剪贴板条目
// ---------------------------------------------------------------------------

/// 已持久化的剪贴板历史条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: ClipboardItemId,
    pub content_type: ContentType,
    /// 文本类型：原始正文。图片类型：图片文件的绝对路径。
    pub content_text: String,
    pub fingerprint: String,
    /// 所属分组 ID（None = 未分组）。替代旧的 is_favorite。
    pub group_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_copied_at: DateTime<Utc>,
    /// 复制来源应用名称（macOS 上自动采集前台应用名）。
    pub source_app: Option<String>,
    /// 复制来源 URL（浏览器复制时自动采集当前标签页 URL）。
    pub source_url: Option<String>,
    pub legacy_id: Option<String>,
}

/// 新增条目输入结构。
#[derive(Debug, Clone)]
pub struct NewClipboardItem {
    pub content_type: ContentType,
    pub content_text: String,
    pub fingerprint: String,
    pub source_app: Option<String>,
    pub source_url: Option<String>,
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

// ---------------------------------------------------------------------------
//  Flow 会话（Phase 1 第 3 步）
// ---------------------------------------------------------------------------

/// 会话的来源。
///
/// 两个值代表两种完全不同的信任级别：`user` 是用户亲手保存的（为什么成群他自己
/// 知道），`agent` 是本地聚类自动产出的（每一行成员都必须说得清为什么，
/// 见 migration 7 里 `reason` 的条件 CHECK）。「重算只替换 agent」也以此为依据。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    User,
    Agent,
}

impl SessionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionSource::User => "user",
            SessionSource::Agent => "agent",
        }
    }

    /// 从库里的字符串还原。认不出来返回 `None`：CHECK 保证只有这两个值，
    /// 真读出第三个说明库被人改过 —— 由调用方决定退路，而不是在这里悄悄糊一个。
    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "user" => Some(SessionSource::User),
            "agent" => Some(SessionSource::Agent),
            _ => None,
        }
    }
}

/// 一条待写入的会话。
///
/// 成员行的 `source` 不在这里：它由 store 取自所属会话，避免「会话是什么来源」
/// 出现第二个真相源。`items` 里的 `position` 从 1 开始（关键判断 12：
/// 界面上的编号就是库里的编号）。
#[derive(Debug, Clone)]
pub struct AgentSessionDraft {
    pub id: String,
    pub source: SessionSource,
    pub title: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub items: Vec<AgentSessionItemDraft>,
}

/// 会话的一条成员行。
#[derive(Debug, Clone)]
pub struct AgentSessionItemDraft {
    pub item_id: String,
    pub position: i64,
    /// `agent` 源必须给且 trim 后非空（数据库 CHECK 也钉着这一条）；
    /// `user` 源不给理由 —— 只有自动成组才需要解释。
    pub reason: Option<String>,
}

/// 列表用的会话摘要。`item_count` 由查询时统计成员行得出，不落列
/// （成员数是派生量，存下来就要在每次增删成员时同步维护）。
#[derive(Debug, Clone)]
pub struct AgentSessionSummary {
    pub id: String,
    pub source: SessionSource,
    pub title: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub item_count: u64,
}

/// 详情里的一条成员：条目本身 + 位置 + 理由。
#[derive(Debug, Clone)]
pub struct AgentSessionMember {
    pub item: ClipboardItem,
    pub position: i64,
    pub reason: Option<String>,
}

/// 会话详情。成员按 `position` 升序，编号直接就是界面上的编号。
#[derive(Debug, Clone)]
pub struct AgentSessionDetail {
    pub id: String,
    pub source: SessionSource,
    pub title: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub members: Vec<AgentSessionMember>,
}

// ---------------------------------------------------------------------------
//  Planner 建议（Phase 1 第 5 步 · Tool Policy 地基）
// ---------------------------------------------------------------------------

/// 一条建议的动作类型。
///
/// 四个值**就是白名单本身**：删除 / 清空 / 改设置这类动作没有对应变体，
/// 模型写出这些词也只能被丢弃。字面量与 `planner_build::PLANNER_ACTION_KEYS`
/// 同一套（`as_str` 与 `from_key` 互逆，测试钉住）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerSuggestionKind {
    /// 复制回系统剪贴板（恰好 1 条目标）。
    Copy,
    /// 粘贴到当前应用（恰好 1 条目标）。
    Paste,
    /// 归入一个分组（多目标；分组由用户在确认卡片里选，不由模型指定）。
    Group,
    /// 交给 AI 处理（多目标；子动作见 `PlannerSuggestion::ai_action`）。
    Ai,
}

impl PlannerSuggestionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlannerSuggestionKind::Copy => "copy",
            PlannerSuggestionKind::Paste => "paste",
            PlannerSuggestionKind::Group => "group",
            PlannerSuggestionKind::Ai => "ai",
        }
    }

    /// 从协议里的 `action` 字面量还原类型。认不出来返回 `None`：白名单外的词
    /// （删除 / 清空 / 改设置）必须落空，由调用方丢弃整行，而不是在这里悄悄
    /// 糊一个别的动作。
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "copy" => Some(PlannerSuggestionKind::Copy),
            "paste" => Some(PlannerSuggestionKind::Paste),
            "group" => Some(PlannerSuggestionKind::Group),
            "ai" => Some(PlannerSuggestionKind::Ai),
            _ => None,
        }
    }
}

/// 一条建议的目标。
///
/// `position` 是会话成员**在库里的** 1-based 编号（界面上的编号就是它），
/// 不是 prompt 里的 `[n]` —— prompt 按「最近优先」重排，两套编号混用会指错条目。
///
/// 字段面与 `AgentSessionMember` 几乎相同，但语义不同：后者是「会话的一个成员」
/// （多一个 `reason`），前者是「一个动作的目标」—— 合成一个类型会让这两件事在
/// 类型上不可区分。
#[derive(Debug, Clone)]
pub struct PlannerTarget {
    pub item: ClipboardItem,
    pub position: i64,
}

/// 一条建议。
///
/// `index` 是这次建议列表里的第几条（1-based，连续无空洞：被丢弃的行不占号）。
/// `ai_action` 存 `AgentAction` 的 key 字符串（`None` = 非 ai 类）：建议的领域
/// 模型不该把 `agent_service` 的动作枚举搬进来，校验时直接问那张真表。
#[derive(Debug, Clone)]
pub struct PlannerSuggestion {
    pub index: u64,
    pub kind: PlannerSuggestionKind,
    pub ai_action: Option<String>,
    pub targets: Vec<PlannerTarget>,
    pub reason: String,
}

/// 一次建议的全部。
///
/// 空列表只有一种含义：模型明确说了「没有值得执行的下一步」。一行都没活下来
/// 且模型没说 `NONE` 时，服务层返回的是错误而不是空集 —— 所以这里**不设**
/// `explicit_none` 字段，两种「空」在服务层就已经分开了。
#[derive(Debug, Clone)]
pub struct PlannerSuggestionSet {
    pub suggestions: Vec<PlannerSuggestion>,
    /// 模型给了但不符合协议、被丢弃的行数（不静默吞掉，如实告知用户）。
    pub dropped: u64,
}

/// 一段对话（用户看得见的派生数据，不是审计）。
#[derive(Debug, Clone)]
pub struct AgentChatSummary {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: u64,
}

/// 对话里的一条可见消息。工具调用不落库，只在当次请求里回灌模型。
#[derive(Debug, Clone)]
pub struct AgentChatMessage {
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// 一段对话的详情。
#[derive(Debug, Clone)]
pub struct AgentChatDetail {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<AgentChatMessage>,
}
