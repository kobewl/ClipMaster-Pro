//! Domain 定义的端口（接口）。Infrastructure 负责实现。

use crate::domain::error::{ClipboardSourceError, RepositoryError, SecretError};
use crate::domain::model::{
    AgentChatDetail, AgentChatSummary, AgentSessionDetail, AgentSessionDraft,
    AgentSessionSummary, ClipGroup, ClipboardItem, ClipboardItemId, ContentType, NewClipboardItem,
};
use crate::domain::settings::AppSettings;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// 每条条目命中了查询里的哪些词（条目 id → 词表，词序同查询词序）。
    ///
    /// 严格与放宽两条路径**都**装配；`query` 为空（浏览列表）时是 `None`——
    /// 那时没有查询词可言，多填一个空 map 只会让前端多分支。
    pub matched_terms: Option<HashMap<String, Vec<String>>>,
    /// 放宽召回后仍没有任何保留条目命中的词（前端横幅用它解释「为什么放宽了」）。
    /// `None` = 本次没走放宽路径。
    pub relaxed_dropped: Option<Vec<String>>,
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

    /// 放宽召回：查询词之间是 **OR** 语义（命中任一即返回），最多 `cap` 条、时间倒序。
    ///
    /// 与 [`ClipboardRepository::search`] 的分工：严格路径（AND、COUNT、分页全在
    /// SQL 里）一行不改；放宽只负责把候选拉回来，够不够格由应用层按命中词数判断。
    /// 实现细节（FTS / LIKE 双查询、筛选条件如何参与）见 SQLite 实现处的注释。
    ///
    /// `filters` 的 `group_id` / `content_type` / `since` 照常生效 ——
    /// 放宽的是**查询词**，不是用户的筛选条件；`limit`/`offset`/`search_text` 不参与。
    async fn search_relaxed(
        &self,
        filters: &SearchQuery,
        terms: &[String],
        cap: u32,
    ) -> Result<Vec<ClipboardItem>, RepositoryError>;

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

    /// 会话候选取数：`last_copied_at >= since` 的最近 `limit` 条，新复制在前。
    ///
    /// **与 [`SearchQuery::since`] 的语义差异是刻意的**（见关键判断 9）：
    /// 那边用 `created_at`（首次入库时间，检索的时间筛选语义），这边用
    /// `last_copied_at` —— 会话按「最近一次复制」成组，同一条内容被再复制一次
    /// 就该回到工作记忆的顶端。既有 `search` 的 `since` 语义一行不改。
    async fn list_since(
        &self,
        since: &str,
        limit: u32,
    ) -> Result<Vec<ClipboardItem>, RepositoryError>;

    /// 按 id 批量取条目（一条 `WHERE id IN (…)`），按 `last_copied_at` 升序
    /// 保证顺序确定；**只返回仍然存在的那些** —— 不存在的 id 由调用方
    /// 与入参比对得出（用户手动保存时条数对不上要如实报错）。
    async fn get_many(&self, ids: &[String]) -> Result<Vec<ClipboardItem>, RepositoryError>;
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
//  AgentConfigStore（模型服务地址等非敏感配置）
// ---------------------------------------------------------------------------

/// 模型服务的连接信息，不含密钥（密钥走 SecretStore）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProviderConfig {
    /// OpenAI 兼容服务的根地址，不带结尾斜杠。
    pub base_url: String,
    pub model: String,
}

#[async_trait]
pub trait AgentConfigStore: Send + Sync {
    /// `Ok(None)` = 用户没配过，调用方回退到默认值。
    async fn load_agent_config(&self) -> Result<Option<AgentProviderConfig>, RepositoryError>;

    async fn save_agent_config(&self, config: AgentProviderConfig)
        -> Result<(), RepositoryError>;
}

// ---------------------------------------------------------------------------
//  AgentRunStore（AI 调用审计）
// ---------------------------------------------------------------------------

/// 一次 AI 调用的审计记录。
///
/// **刻意不存的东西**：prompt 正文、模型响应正文、API Key。前两者本来就在
/// 剪贴板历史里，审计再存一份等于把隐私面翻倍；密钥从不落库。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunRecord {
    pub id: String,
    pub created_at: String,
    /// 机器可读的动作名（`summarize` / `translate_zh` / …），不是界面文案 ——
    /// 界面文案会改，改完历史数据的口径就对不上了。
    pub action: String,
    /// 请求实际发往的服务。`None` = 在本地就被拦下，根本没到达任何服务。
    pub provider: Option<String>,
    pub model: Option<String>,
    /// 本次调用读了哪些剪贴板条目。至少一条，否则这条记录没有主语。
    pub input_item_ids: Vec<String>,
    pub input_chars: u64,
    /// `"ok"` 或 `"error"`。
    pub status: String,
    pub error_code: Option<String>,
    pub duration_ms: u64,
    pub output_chars: Option<u64>,
}

#[async_trait]
pub trait AgentRunStore: Send + Sync {
    /// 落一条审计。实现方应保证：没有主语的记录不留在表里。
    async fn record(&self, run: AgentRunRecord) -> Result<(), RepositoryError>;
    /// 最近 N 条，新的在前。
    async fn list_recent(&self, limit: u32) -> Result<Vec<AgentRunRecord>, RepositoryError>;
    /// 按 ID 取一条（结果卡片的「查看来源」用）。
    async fn find(&self, id: &str) -> Result<Option<AgentRunRecord>, RepositoryError>;
    /// 清空全部审计，返回删掉的条数。
    async fn clear(&self) -> Result<u64, RepositoryError>;
}

// ---------------------------------------------------------------------------
//  AgentSessionStore（Flow 会话派生数据）
// ---------------------------------------------------------------------------

/// 会话派生数据的读写端口。
///
/// 两条不变量由实现方负责：`source='agent'` 的会话由重算**整体替换**（单事务），
/// `source='user'` 的会话只受用户自己的删除影响（重算与自动裁剪都不碰它）。
#[async_trait]
pub trait AgentSessionStore: Send + Sync {
    /// 重算的落库动作：**单事务**内先删掉全部 `source='agent'` 会话，再写入
    /// `sessions`，返回写入的会话条数。会话 id 由成员集合派生（见关键判断 4），
    /// 所以未变化的会话在替换前后是同一行 —— 重算因此是幂等的。
    async fn replace_agent_sessions(
        &self,
        sessions: Vec<AgentSessionDraft>,
    ) -> Result<u64, RepositoryError>;

    /// 写一条会话。只用于用户显式保存（`source='user'`）。
    async fn insert_session(&self, session: AgentSessionDraft) -> Result<(), RepositoryError>;

    /// 最近 `limit` 条会话，新的在前（`updated_at DESC`），带成员计数。
    async fn list(&self, limit: u32) -> Result<Vec<AgentSessionSummary>, RepositoryError>;

    /// 按 id 取详情，成员按 `position ASC` 排。`Ok(None)` = 会话已不在
    /// （条目被删到一条不剩时由触发器带走），属正常状态而不是错误。
    async fn find(&self, id: &str) -> Result<Option<AgentSessionDetail>, RepositoryError>;

    /// 删一条会话（成员行由外键级联消失）。返回是否真的删掉一条。
    async fn delete(&self, id: &str) -> Result<bool, RepositoryError>;

    /// 清空全部会话，返回删掉的条数。
    ///
    /// **含 `source='user'`**：这是用户显式要求的「清除所有 AI 派生数据」，
    /// 用户手动保存的会话同样是派生数据（原始剪贴板记录不受影响）。
    async fn clear(&self) -> Result<u64, RepositoryError>;
}

// ---------------------------------------------------------------------------
//  AgentChatStore（对话派生数据）
// ---------------------------------------------------------------------------

/// 对话的读写端口。正文只存在这里，审计表仍然不存 prompt / 响应。
#[async_trait]
pub trait AgentChatStore: Send + Sync {
    async fn insert(
        &self,
        id: &str,
        title: &str,
        created_at: &str,
        updated_at: &str,
    ) -> Result<(), RepositoryError>;

    async fn append_message(
        &self,
        chat_id: &str,
        role: &str,
        content: &str,
        created_at: &str,
    ) -> Result<(), RepositoryError>;

    async fn touch(&self, chat_id: &str, title: Option<&str>, updated_at: &str)
        -> Result<(), RepositoryError>;

    async fn list(&self, limit: u32) -> Result<Vec<AgentChatSummary>, RepositoryError>;

    async fn find(&self, id: &str) -> Result<Option<AgentChatDetail>, RepositoryError>;

    async fn delete(&self, id: &str) -> Result<bool, RepositoryError>;

    async fn clear(&self) -> Result<u64, RepositoryError>;
}

// ---------------------------------------------------------------------------
//  SecretStore（模型密钥等敏感配置）
// ---------------------------------------------------------------------------

/// 系统安全存储的端口，macOS 上由钥匙串实现。
///
/// 独立于 `SettingsStore`：设置表是明文 SQLite，API Key 存进去等于泄露。
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// `Ok(None)` = 该项不存在，属正常状态。
    async fn get(&self, account: &str) -> Result<Option<String>, SecretError>;
    async fn set(&self, account: &str, value: &str) -> Result<(), SecretError>;
    /// 幂等：条目不存在也返回成功。
    async fn delete(&self, account: &str) -> Result<(), SecretError>;
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
