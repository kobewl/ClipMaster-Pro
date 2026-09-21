//! 剪贴板 Agent 的门面：读取内容、脱敏门禁、构建提示词、调用模型。
//!
//! UI 不直接调用模型，只能提交已存在的剪贴板 ID 和受控 action。
//! 模型服务默认 DeepSeek，用户可在设置里改成任意 OpenAI 兼容端点。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::application::agent_prompt::{prepare_prompt, AgentInput, PromptTask, TOTAL_BUDGET_CHARS};
use crate::application::history_service::HistoryService;
use crate::domain::model::ContentType;
use crate::domain::normalize::strip_html_tags;
use crate::domain::ports::{
    AgentConfigStore, AgentProviderConfig, AgentRunRecord, AgentRunStore, SecretStore,
};

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
/// DeepSeek 官方当前默认模型 ID（DeepSeek-V4.1-Flash）。
/// 见 https://api-docs.deepseek.com/quick_start/pricing —— 旧名 `deepseek-chat` 已退役。
pub const DEFAULT_MODEL: &str = "deepseek-flash";

/// 一次 AI 调用最多处理多少条记录。
///
/// 这是**用户可理解的**上限，不是模型的限制（预算分配另有算法）。
/// 选 200 条让 Agent 归纳本身就是个模糊的需求，与其静默丢掉大部分、
/// 给一个看不出根据的结论，不如明确告诉用户先缩小范围。
pub const MAX_AGENT_INPUT_ITEMS: usize = 20;

/// 钥匙串里存 API Key 用的 account 名（service 由实现方固定为 bundle id）。
pub const SECRET_ACCOUNT_AI: &str = "api-key";

/// 开发期回退用；优先级低于设置里保存的值。
const ENV_API_KEY: &str = "CLIPMASTER_AGENT_API_KEY";
const ENV_BASE_URL: &str = "CLIPMASTER_AGENT_BASE_URL";
const ENV_MODEL: &str = "CLIPMASTER_AGENT_MODEL";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAction {
    Summarize,
    TranslateZh,
    Explain,
    ExtractTasks,
    FormatJson,
}

impl AgentAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Summarize => "总结",
            Self::TranslateZh => "翻译为中文",
            Self::Explain => "解释",
            Self::ExtractTasks => "提取待办",
            Self::FormatJson => "格式化 JSON",
        }
    }

    /// 机器可读的动作名，审计表里存这个。界面文案会改，改完历史数据的口径就对不上了。
    pub fn key(&self) -> &'static str {
        match self {
            Self::Summarize => "summarize",
            Self::TranslateZh => "translate_zh",
            Self::Explain => "explain",
            Self::ExtractTasks => "extract_tasks",
            Self::FormatJson => "format_json",
        }
    }

    /// 从审计表里的 key 还原动作。认不出来返回 None —— 宁可显示原始字符串，
    /// 也不要糊一个错的动作名上去。
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "summarize" => Some(Self::Summarize),
            "translate_zh" => Some(Self::TranslateZh),
            "explain" => Some(Self::Explain),
            "extract_tasks" => Some(Self::ExtractTasks),
            "format_json" => Some(Self::FormatJson),
            _ => None,
        }
    }

    fn instruction(&self) -> &'static str {
        match self {
            Self::Summarize => "用中文总结要点。保留事实、数字、结论和未决问题；不要编造内容。",
            Self::TranslateZh => "翻译为简体中文。保留 Markdown、代码、URL、专有名词和原有结构。",
            Self::Explain => "用中文解释这段内容。先说明它是什么，再解释关键术语、风险和下一步；不要臆测上下文。",
            Self::ExtractTasks => "提取明确或合理推断的待办事项。用 Markdown checklist 输出；没有待办时明确写“未发现明确待办”。",
            Self::FormatJson => "将内容校验并格式化为合法 JSON，只输出格式化后的 JSON，不要加解释。若输入不是合法 JSON，简短说明错误位置和原因。",
        }
    }

    /// 多条目场景的指令。与单条的差别不只是"复数"：
    /// 单条是"读懂这一条"，多条目是"读懂它们之间的关系"，后者要求
    /// 跨记录归纳而不是逐条复述 —— 否则给了 5 条内容，拿回来 5 段独立摘要，
    /// 用户自己也能做，用不着模型。
    fn batch_instruction(&self) -> &'static str {
        match self {
            Self::Summarize => {
                "这是一组内容，请做跨记录的归纳：它们共同在讲什么、彼此印证或冲突的地方、\
                 以及还没解决的问题。不要逐条复述 —— 逐条摘要用户自己就能看。"
            }
            Self::TranslateZh => {
                "把每一条分别翻译为简体中文，保留各自的编号。\
                 保留 Markdown、代码、URL、专有名词和原有结构。"
            }
            Self::Explain => {
                "把这一组内容当作整体来解释：它们共同的主题是什么、涉及哪些关键术语、\
                 有哪些风险或未解的疑点。不要把每条单独解释一遍。"
            }
            Self::ExtractTasks => {
                "汇总这一组内容里的待办事项，把重复的合并成一条。\
                 用 Markdown checklist 输出；没有待办时明确写“未发现明确待办”。"
            }
            // format_json 对一组内容没有意义：把五段各自合法的 JSON 拼在一起
            // 不是合法 JSON，分别格式化又等于做了五次单条操作。
            Self::FormatJson => {
                "格式化 JSON 这个动作只对单条内容有意义，请只选中一条再运行。"
            }
        }
    }

    /// 这个动作能不能对一组内容运行。
    pub fn supports_batch(&self) -> bool {
        !matches!(self, Self::FormatJson)
    }
}

/// 一条输入在这次调用里的实际处理情况。
///
/// 把"用了多少、有没有被截断"回传给界面，是为了让用户能核对结论的依据：
/// 结果说"三条记录都提到了超时"，用户得能看出第三条其实只送了开头三分之一。
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentInputReport {
    /// 1 起的编号，与结果正文里的 `[n]` 引用对应。
    pub index: usize,
    pub item_id: String,
    pub source: String,
    /// 原文长度（截断前）。
    pub full_chars: u64,
    /// 实际发给模型的长度。
    pub used_chars: u64,
    pub truncated: bool,
    /// 内容里有疑似 prompt injection 的句式，已被标记为纯数据。
    pub suspicious: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentResult {
    pub request_id: String,
    pub action: String,
    pub title: String,
    pub content: String,
    pub provider: String,
    pub model: String,
    pub source_item_ids: Vec<String>,
    /// 每条输入的处理情况，编号与正文里的引用一致。
    pub inputs: Vec<AgentInputReport>,
    /// 因为超出条数上限而整条没送进模型的记录（最旧的先丢）。
    pub dropped_item_ids: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("尚未配置 DeepSeek API Key，请到「设置 → AI 助手」填入。")]
    NotConfigured,
    /// 带上位置：一次选了多条时，"有一条含密钥"和"是第 3 条含密钥"对用户
    /// 是完全不同的信息量 —— 后者让他知道把哪条去掉就能继续。
    #[error("选择中的第 {position} 条内容里检测到可能的 Token、密码或私钥，已阻止整批发送到云端。")]
    SensitiveContent { position: usize },
    #[error("AI Actions 暂只支持文本和 HTML 内容。")]
    UnsupportedContent,
    #[error("一次最多处理 {MAX_AGENT_INPUT_ITEMS} 条记录，当前选中了 {count} 条，请缩小范围后重试。")]
    TooManyItems { count: usize },
    #[error("这个动作只能对单条内容运行，请只选中一条再试。")]
    BatchUnsupportedAction,
    #[error("内容过长（最多 {} 个字符），请先裁剪后再运行 AI Action。", TOTAL_BUDGET_CHARS)]
    InputTooLong,
    #[error("DeepSeek 拒绝了这次请求（API Key 可能无效或已被撤销），请到「设置 → AI 助手」检查。")]
    Unauthorized,
    #[error("DeepSeek 账户余额不足，请充值后重试。")]
    InsufficientBalance,
    #[error("DeepSeek 请求过于频繁，请稍后重试。")]
    RateLimited,
    #[error("DeepSeek 服务暂时不可用，请稍后重试。")]
    ProviderUnavailable,
    #[error("DeepSeek 返回了无法识别的结果，请稍后重试。")]
    InvalidResponse,
    #[error("无法读取这条剪贴板记录。")]
    ItemUnavailable,
    #[error("API Key 校验失败：{0}")]
    InvalidApiKey(String),
    #[error("服务地址不可用：{0}")]
    InvalidBaseUrl(String),
    #[error("模型名校验失败：{0}")]
    InvalidModel(String),
    #[error("无法访问系统钥匙串：{0}")]
    SecretStoreUnavailable(String),
    #[error("无法保存模型服务配置：{0}")]
    ConfigStoreUnavailable(String),
    #[error("无法读写 AI 使用记录：{0}")]
    RunStoreUnavailable(String),
}

impl AgentError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "ai_not_configured",
            Self::SensitiveContent { .. } => "ai_sensitive_content",
            Self::UnsupportedContent => "ai_unsupported_content",
            Self::TooManyItems { .. } => "ai_too_many_items",
            Self::BatchUnsupportedAction => "ai_batch_unsupported_action",
            Self::InputTooLong => "ai_input_too_long",
            Self::Unauthorized => "ai_unauthorized",
            Self::InsufficientBalance => "ai_insufficient_balance",
            Self::RateLimited => "ai_rate_limited",
            Self::ProviderUnavailable => "ai_provider_unavailable",
            Self::InvalidResponse => "ai_invalid_response",
            Self::ItemUnavailable => "ai_item_unavailable",
            Self::InvalidApiKey(_) => "ai_invalid_api_key",
            Self::InvalidBaseUrl(_) => "ai_invalid_base_url",
            Self::InvalidModel(_) => "ai_invalid_model",
            Self::SecretStoreUnavailable(_) => "secret_unavailable",
            Self::ConfigStoreUnavailable(_) => "ai_config_save_failed",
            Self::RunStoreUnavailable(_) => "ai_run_store_unavailable",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::ProviderUnavailable | Self::InvalidResponse | Self::RateLimited
        )
    }
}

/// 当前生效的模型配置。`api_key` 只在内存里流转，**永远不进数据库和日志**。
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    /// Key 来自环境变量而非钥匙串（设置面板据此提示用户）。
    pub api_key_from_env: bool,
}

/// 这次调用是单条还是多条。两者在输入门禁与 prompt 指令上都不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Single,
    Batch,
}

/// `run` 执行过程中逐步补齐的审计上下文：走到哪一步，才知道哪些字段有真值。
#[derive(Default)]
struct RunAudit {
    /// 本次调用的 ID。**和结果卡片上的 request_id 是同一个值** —— 用户在界面上
    /// 看到的那个号，就是审计表里的那一行，可追溯才是真的可追溯。
    request_id: Option<String>,
    /// 本次要发往的服务。本地就被拦下时保持 `None` —— 什么都没发出去。
    target: Option<(String, String)>,
    input_chars: u64,
    /// 送进 prompt 的条目。请求发出前就失败了的话，这里记的是用户选中的全部
    /// —— "这次本来要处理哪些内容"同样值得留下。
    input_item_ids: Vec<String>,
    dropped_item_ids: Vec<String>,
}

/// 给前端的配置快照，**不含密钥本体**。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentConfigInfo {
    pub configured: bool,
    pub key_hint: Option<String>,
    pub key_source: String,
    pub base_url: String,
    pub model: String,
    /// 地址是用户设的（false = 内置默认值）。
    pub base_url_is_custom: bool,
    /// 界面上显示的服务名（默认地址显示 "DeepSeek"，自定义显示真实主机名）。
    /// 由后端算而不是前端拼：结果卡片上的来源标注走的也是这个函数，
    /// 两处必须一致 —— 否则标题写着 DeepSeek、结果卡片写着中转站。
    pub provider_label: String,
}

pub struct AgentService {
    history: Arc<HistoryService>,
    secrets: Arc<dyn SecretStore>,
    config_store: Arc<dyn AgentConfigStore>,
    runs: Arc<dyn AgentRunStore>,
    client: reqwest::Client,
}

impl AgentService {
    pub fn new(
        history: Arc<HistoryService>,
        secrets: Arc<dyn SecretStore>,
        config_store: Arc<dyn AgentConfigStore>,
        runs: Arc<dyn AgentRunStore>,
    ) -> Self {
        Self {
            history,
            secrets,
            config_store,
            runs,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(45))
                .build()
                // 没有 client 时所有请求都会失败；继续让 run 映射成稳定错误。
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// 生效配置：用户设置 → 环境变量 → 内置默认值。
    pub async fn config(&self) -> Result<AgentConfig, AgentError> {
        let stored = self
            .secrets
            .get(SECRET_ACCOUNT_AI)
            .await
            .map_err(|err| AgentError::SecretStoreUnavailable(err.to_string()))?
            .filter(|key| !key.trim().is_empty());
        let from_env = stored.is_none();
        let api_key = stored.or_else(|| {
            std::env::var(ENV_API_KEY)
                .ok()
                .filter(|key| !key.trim().is_empty())
        });

        let endpoint = self.resolve_endpoint().await;

        Ok(AgentConfig {
            api_key,
            base_url: endpoint.base_url,
            model: endpoint.model,
            api_key_from_env: from_env,
        })
    }

    async fn resolve_endpoint(&self) -> AgentProviderConfig {
        let saved = match self.config_store.load_agent_config().await {
            Ok(saved) => saved,
            Err(err) => {
                // 读配置失败不该让 AI 功能整体瘫掉。
                tracing::warn!(error = %err, "读取模型服务配置失败，回退到默认值");
                None
            }
        };
        if let Some(saved) = saved {
            return migrate_legacy_deepseek_model(saved);
        }
        AgentProviderConfig {
            base_url: env_base_url().unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: env_model().unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        }
    }

    /// 配置快照，密钥只给掩码。
    pub async fn config_info(&self) -> Result<AgentConfigInfo, AgentError> {
        let config = self.config().await?;
        Ok(AgentConfigInfo {
            configured: config.api_key.is_some(),
            key_hint: config.api_key.as_deref().map(mask_api_key),
            key_source: if config.api_key_from_env {
                "env".to_string()
            } else {
                "keychain".to_string()
            },
            // 按值判断而不是"库里有没有记录"：用户点过「恢复默认」之后
            // 库里确实存着一条，但那条就是默认值，不该显示成"自定义"。
            base_url_is_custom: config.base_url != DEFAULT_BASE_URL,
            provider_label: provider_label(&config.base_url),
            base_url: config.base_url,
            model: config.model,
        })
    }

    /// 保存服务地址与模型名。校验在后端做：地址会被拼进请求，前端那层只是即时反馈。
    pub async fn save_endpoint(
        &self,
        base_url: &str,
        model: &str,
    ) -> Result<AgentConfigInfo, AgentError> {
        let base_url = normalize_base_url(base_url).map_err(AgentError::InvalidBaseUrl)?;
        let model = model.trim();
        if model.is_empty() {
            return Err(AgentError::InvalidModel("模型名不能为空。".to_string()));
        }
        if model.chars().any(char::is_whitespace) {
            return Err(AgentError::InvalidModel(
                "模型名不能包含空格或换行。".to_string(),
            ));
        }
        self.config_store
            .save_agent_config(AgentProviderConfig {
                base_url,
                model: model.to_string(),
            })
            .await
            .map_err(|err| AgentError::ConfigStoreUnavailable(err.to_string()))?;
        self.config_info().await
    }

    /// 恢复内置默认地址与模型。
    pub async fn reset_endpoint(&self) -> Result<AgentConfigInfo, AgentError> {
        self.config_store
            .save_agent_config(AgentProviderConfig {
                base_url: DEFAULT_BASE_URL.to_string(),
                model: DEFAULT_MODEL.to_string(),
            })
            .await
            .map_err(|err| AgentError::ConfigStoreUnavailable(err.to_string()))?;
        self.config_info().await
    }

    /// 保存 Key 到系统钥匙串。先校验格式，避免把一个连不到服务的前缀存进去。
    pub async fn save_api_key(&self, raw: &str) -> Result<AgentConfigInfo, AgentError> {
        let key = raw.trim();
        validate_api_key(key).map_err(AgentError::InvalidApiKey)?;
        // 去零宽字符：从网页复制 Key 时很容易带上，肉眼完全看不出来。
        let cleaned: String = key.chars().filter(|ch| !is_invisible(*ch)).collect();
        self.secrets
            .set(SECRET_ACCOUNT_AI, &cleaned)
            .await
            .map_err(|err| AgentError::SecretStoreUnavailable(err.to_string()))?;
        self.config_info().await
    }

    /// 清除已保存的 Key。开发期环境变量的那份不受影响（它本来就不归应用管）。
    pub async fn clear_api_key(&self) -> Result<AgentConfigInfo, AgentError> {
        self.secrets
            .delete(SECRET_ACCOUNT_AI)
            .await
            .map_err(|err| AgentError::SecretStoreUnavailable(err.to_string()))?;
        self.config_info().await
    }

    /// 「测试连接」：用最小代价证明 Key 与网络都通（`GET /models`，不产生推理费用）。
    pub async fn test_connection(&self) -> Result<(), AgentError> {
        let config = self.config().await?;
        let api_key = config.api_key.ok_or(AgentError::NotConfigured)?;
        let response = self
            .client
            .get(format!("{}/models", config.base_url))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|_| AgentError::ProviderUnavailable)?;
        if response.status().is_success() {
            return Ok(());
        }
        Err(map_status_error(response.status()))
    }

    /// 单条内容的 AI 动作（Phase 0 契约，行为不变）。
    pub async fn run(&self, item_id: &str, action: AgentAction) -> Result<AgentResult, AgentError> {
        self.run_items(&[item_id.to_string()], action, InputMode::Single)
            .await
    }

    /// 多条内容的 AI 动作：跨记录归纳。
    pub async fn run_many(
        &self,
        item_ids: &[String],
        action: AgentAction,
    ) -> Result<AgentResult, AgentError> {
        self.run_items(item_ids, action, InputMode::Batch).await
    }

    async fn run_items(
        &self,
        item_ids: &[String],
        action: AgentAction,
        mode: InputMode,
    ) -> Result<AgentResult, AgentError> {
        let started = std::time::Instant::now();
        let mut audit = RunAudit {
            // 先定号：结果卡片要显示它，审计也要用它当主键。
            request_id: Some(uuid::Uuid::new_v4().to_string()),
            ..Default::default()
        };
        let result = self.run_inner(item_ids, &action, mode, &mut audit).await;
        self.audit(&action, &result, audit, started.elapsed()).await;
        result
    }

    async fn run_inner(
        &self,
        item_ids: &[String],
        action: &AgentAction,
        mode: InputMode,
        audit: &mut RunAudit,
    ) -> Result<AgentResult, AgentError> {
        if item_ids.is_empty() {
            return Err(AgentError::ItemUnavailable);
        }
        if item_ids.len() > MAX_AGENT_INPUT_ITEMS {
            return Err(AgentError::TooManyItems { count: item_ids.len() });
        }
        if mode == InputMode::Batch && !action.supports_batch() {
            return Err(AgentError::BatchUnsupportedAction);
        }
        // 先把"用户选中了什么"记进审计：后面任何一步失败，这条线索都在。
        audit.input_item_ids = item_ids.to_vec();

        // 逐条读取并做本地门禁。这里刻意**先全部读完再发请求**：
        // 一批内容里只要有任意一条敏感，就不该有任何一条被发出去。
        let mut inputs: Vec<AgentInput> = Vec::with_capacity(item_ids.len());
        for (position, item_id) in item_ids.iter().enumerate() {
            let item = self
                .history
                .get(item_id)
                .await
                .map_err(|_| AgentError::ItemUnavailable)?;
            let text = match item.content_type {
                ContentType::Text => item.content_text,
                ContentType::Html => strip_html_tags(&item.content_text),
                // 图片 / 文件没有可送模型的文本。批量时跳过（界面上已经提示
                // "非文本内容会被跳过"），单条时明确报错。
                ContentType::Image | ContentType::Files => {
                    if mode == InputMode::Single {
                        return Err(AgentError::UnsupportedContent);
                    }
                    continue;
                }
            };
            if contains_sensitive_content(&text) {
                return Err(AgentError::SensitiveContent { position: position + 1 });
            }
            // 单条路径保持 Phase 0 的约定：超长直接报错让用户先裁剪，
            // 而不是悄悄截断后给出一个基于残缺内容的结论。
            // 批量路径相反 —— 用户选的是一组，不能因为其中一条长就整批失败，
            // 所以按预算截断并在结果里逐条注明。
            if mode == InputMode::Single
                && text.chars().count() > crate::application::agent_prompt::TOTAL_BUDGET_CHARS
            {
                return Err(AgentError::InputTooLong);
            }
            inputs.push(AgentInput {
                item_id: item_id.clone(),
                text,
                source_app: item.source_app,
                content_type: item.content_type.as_str().to_string(),
                last_copied_at: item.last_copied_at,
            });
        }

        if inputs.is_empty() {
            // 整批都是图片 / 文件：没有任何内容可以送给模型。
            return Err(AgentError::UnsupportedContent);
        }

        let prepared = prepare_prompt(
            PromptTask {
                label: action.label(),
                instruction: if mode == InputMode::Batch {
                    action.batch_instruction()
                } else {
                    action.instruction()
                },
                cite_sources: mode == InputMode::Batch,
            },
            &inputs,
        );

        audit.input_chars = prepared.used.iter().map(|used| used.used_chars as u64).sum();
        audit.input_item_ids = prepared.used.iter().map(|used| used.item_id.clone()).collect();
        audit.dropped_item_ids = prepared.dropped.clone();

        let config = self.config().await?;
        // 配置解出来就记下目标服务：即使后面因为没配 Key 或网络失败没读成，
        // 审计里也能看出"这次本来要发往哪里"。
        audit.target = Some((provider_label(&config.base_url), config.model.clone()));
        let api_key = config.api_key.ok_or(AgentError::NotConfigured)?;

        let response = self
            .client
            .post(format!("{}/chat/completions", config.base_url))
            .bearer_auth(api_key)
            .json(&serde_json::json!({
                "model": config.model,
                "temperature": 0.2,
                "messages": [
                    {"role": "system", "content": prepared.system},
                    {"role": "user", "content": prepared.user}
                ]
            }))
            .send()
            .await
            .map_err(|_| AgentError::ProviderUnavailable)?;

        if !response.status().is_success() {
            let status = response.status();
            // 只记状态码和主机名：地址由用户配置，正文里可能有敏感内容。
            tracing::warn!(status = %status, host = %host_of(&config.base_url), "模型请求失败");
            return Err(map_status_error(status));
        }
        let body: ChatCompletion = response
            .json()
            .await
            .map_err(|_| AgentError::InvalidResponse)?;
        let content = body
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or(AgentError::InvalidResponse)?;

        let label = action.label();
        Ok(AgentResult {
            request_id: audit
                .request_id
                .clone()
                .expect("run 入口已经把 request_id 放进 audit"),
            action: label.to_string(),
            title: if prepared.used.len() > 1 {
                format!("AI {label} · {} 条", prepared.used.len())
            } else {
                format!("AI {label}")
            },
            content,
            provider: provider_label(&config.base_url),
            model: config.model,
            source_item_ids: prepared.used.iter().map(|used| used.item_id.clone()).collect(),
            inputs: prepared
                .used
                .iter()
                .map(|used| AgentInputReport {
                    index: used.index,
                    item_id: used.item_id.clone(),
                    source: used.source.clone(),
                    full_chars: used.full_chars as u64,
                    used_chars: used.used_chars as u64,
                    truncated: used.truncated,
                    suspicious: used.suspicious,
                })
                .collect(),
            dropped_item_ids: prepared.dropped,
        })
    }

    /// 写一条调用审计。
    ///
    /// 审计是旁路：写失败只记日志，不能把用户这次操作本身带崩。
    /// 被本地门禁拦下的调用同样留痕 —— "哪些内容触发过安全规则"本身就是
    /// 用户需要的答案，只在成功时记录等于把最有用的那一半丢掉。
    async fn audit(
        &self,
        action: &AgentAction,
        result: &Result<AgentResult, AgentError>,
        audit: RunAudit,
        elapsed: std::time::Duration,
    ) {
        let (status, error_code, output_chars) = match result {
            Ok(res) => ("ok", None, Some(res.content.chars().count() as u64)),
            Err(err) => ("error", Some(err.code().to_string()), None),
        };
        let (provider, model) = match audit.target {
            Some((provider, model)) => (Some(provider), Some(model)),
            None => (None, None),
        };
        let record = AgentRunRecord {
            // 与结果卡片上的 request_id 同号：用户拿着界面上那个号就能查到这一行。
            id: audit
                .request_id
                .clone()
                .expect("run 入口已经把 request_id 放进 audit"),
            created_at: chrono::Utc::now().to_rfc3339(),
            action: action.key().to_string(),
            provider,
            model,
            // 条目在读它的时候还在（ItemUnavailable 的情况本来就没条目可挂，
            // store 会把这条没有主语的记录丢掉）。
            input_item_ids: audit.input_item_ids,
            input_chars: audit.input_chars,
            status: status.to_string(),
            error_code,
            duration_ms: elapsed.as_millis() as u64,
            output_chars,
        };
        if !audit.dropped_item_ids.is_empty() {
            // 丢了内容这件事必须留痕：结果看起来一切正常，但其实是基于
            // 用户选的一部分得出的 —— 事后复盘时这是最关键的一条线索。
            tracing::warn!(
                dropped = audit.dropped_item_ids.len(),
                "本次调用有内容因超出条数上限未发送"
            );
        }
        if let Err(err) = self.runs.record(record).await {
            tracing::warn!(error = %err, "写入 AI 调用审计失败");
        }
    }

    /// 最近的 AI 调用审计，新的在前。
    pub async fn recent_runs(&self, limit: u32) -> Result<Vec<AgentRunRecord>, AgentError> {
        self.runs
            .list_recent(limit)
            .await
            .map_err(|err| AgentError::RunStoreUnavailable(err.to_string()))
    }

    /// 按 ID 取一条审计（结果卡片的「查看来源」）。找不到返回 `None` ——
    /// 记录可能已经随原始条目被删掉了，那是正常状态，不是错误。
    pub async fn find_run(&self, id: &str) -> Result<Option<AgentRunRecord>, AgentError> {
        self.runs
            .find(id)
            .await
            .map_err(|err| AgentError::RunStoreUnavailable(err.to_string()))
    }

    /// 清空审计记录（设置里的「清除 AI 使用记录」）。
    pub async fn clear_runs(&self) -> Result<u64, AgentError> {
        self.runs
            .clear()
            .await
            .map_err(|err| AgentError::RunStoreUnavailable(err.to_string()))
    }
}

/// 把 HTTP 状态映射成前端能分头处理的错误。
///
/// 401/402/429 各自有明确的自救动作（换 Key / 充值 / 等一会儿），
/// 全都压成「服务暂时不可用」等于把所有情况都推给"稍后重试"。
fn map_status_error(status: reqwest::StatusCode) -> AgentError {
    match status.as_u16() {
        401 | 403 => AgentError::Unauthorized,
        402 => AgentError::InsufficientBalance,
        429 => AgentError::RateLimited,
        _ => AgentError::ProviderUnavailable,
    }
}

/// 服务地址：只认 HTTPS，避免 Key 走明文链接发出去（架构文档安全约束）。
fn env_base_url() -> Option<String> {
    let raw = std::env::var(ENV_BASE_URL).ok()?;
    match normalize_base_url(&raw) {
        Ok(url) => Some(url),
        Err(reason) => {
            tracing::warn!(%reason, "忽略非法的 CLIPMASTER_AGENT_BASE_URL，回退到默认地址");
            None
        }
    }
}

fn env_model() -> Option<String> {
    std::env::var(ENV_MODEL)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 校验并规范化服务地址。地址决定剪贴板内容发往哪里，必须严把关。
///
/// 远程只允许 https；回环地址放宽到 http —— ollama / vLLM 本地部署默认就是明文，
/// 且流量不出本机。不接受内嵌凭据、查询串和片段。
fn normalize_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("服务地址不能为空。".to_string());
    }
    // 结尾斜杠要吃掉，否则会拼出 //chat/completions
    let candidate = trimmed.trim_end_matches('/');

    let parsed = url::Url::parse(candidate)
        .map_err(|_| "地址格式不正确，请填完整地址，例如 https://api.deepseek.com".to_string())?;

    match parsed.scheme() {
        "https" => {}
        "http" => {
            let host = parsed.host_str().unwrap_or_default();
            if !is_loopback_host(host) {
                return Err(format!(
                    "出于安全考虑，非本机地址必须使用 https://（{host} 不是本机地址）。\
                     本地部署的模型服务可以直接填 http://127.0.0.1:端口。"
                ));
            }
        }
        other => {
            return Err(format!("不支持的协议 \"{other}\"，请使用 https://"));
        }
    }

    if parsed.host_str().map(str::is_empty).unwrap_or(true) {
        return Err("地址里缺少主机名。".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("地址里不能包含用户名或密码，请只填服务地址。".to_string());
    }
    if parsed.query().is_some() {
        return Err("地址里不能带查询参数（? 后面的内容）。".to_string());
    }
    if parsed.fragment().is_some() {
        return Err("地址里不能带 # 片段。".to_string());
    }

    Ok(candidate.to_string())
}

/// 取主机名，用于日志与结果卡片的来源标注。
pub fn host_of(base_url: &str) -> String {
    url::Url::parse(base_url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_string()))
        .unwrap_or_default()
}

/// 把仍写在库里的旧 DeepSeek 模型名映射到官方现行 ID。
/// 只动默认 DeepSeek 端点；用户自定义的第三方模型名原样保留。
fn migrate_legacy_deepseek_model(config: AgentProviderConfig) -> AgentProviderConfig {
    let base = config.base_url.trim_end_matches('/');
    if base != DEFAULT_BASE_URL {
        return config;
    }
    let model = match config.model.as_str() {
        "deepseek-chat" | "deepseek-reasoner" => DEFAULT_MODEL.to_string(),
        other => other.to_string(),
    };
    AgentProviderConfig {
        base_url: config.base_url,
        model,
    }
}

/// 结果卡片上显示"内容发到了哪"。自定义地址一律报真实主机名，不能糊弄成 "AI"。
fn provider_label(base_url: &str) -> String {
    if base_url == DEFAULT_BASE_URL {
        return "DeepSeek".to_string();
    }
    let host = host_of(base_url);
    if host.is_empty() {
        "自定义服务".to_string()
    } else {
        host
    }
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") || host == "::1" {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Key 的格式门禁。规则刻意宽松 —— 只拦"明显不像 Key"的输入，
/// 不去猜 DeepSeek 的未来格式，真正有效性的判断交给「测试连接」。
pub fn validate_api_key(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("API Key 不能为空。".to_string());
    }
    if key.chars().count() < 8 {
        return Err("API Key 太短，请确认复制完整。".to_string());
    }
    if key.chars().any(char::is_whitespace) {
        return Err("API Key 不能包含空格或换行。".to_string());
    }
    if !key.is_ascii() {
        return Err("API Key 应只包含 ASCII 字符，请确认没有复制到中文标点。".to_string());
    }
    Ok(())
}

/// 掩码：只留尾部 4 位。设置面板要能分辨"存的是哪把 Key"，
/// 但不能把完整密钥回传给前端 —— 页面状态、devtools、截图都会带上它。
fn mask_api_key(key: &str) -> String {
    let tail: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("••••••••{tail}")
}

/// 零宽字符：从网页/聊天工具复制 Key 时的常见夹带，肉眼不可见但会让认证失败。
fn is_invisible(ch: char) -> bool {
    matches!(
        ch,
        '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}'
    )
}

#[derive(Debug, Deserialize)]
struct ChatCompletion {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

/// 这是“宁可多拦”的本地第一道门，而不是完整 DLP。Phase 1 会将它独立为
/// Policy Engine 并增加用户可配置规则。当前实现刻意不用内容进日志。
fn contains_sensitive_content(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("-----begin") && lower.contains("private key-----") {
        return true;
    }
    if lower.contains("password=") || lower.contains("passwd=") || lower.contains("api_key=") {
        return true;
    }
    if text.contains("sk-") || text.contains("AKIA") || text.contains("xoxb-") {
        return true;
    }
    text.split_whitespace().any(looks_like_jwt)
}

fn looks_like_jwt(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(a) = parts.next() else { return false };
    let Some(b) = parts.next() else { return false };
    let Some(c) = parts.next() else { return false };
    parts.next().is_none()
        && a.len() >= 10
        && b.len() >= 10
        && c.len() >= 10
        && [a, b, c].into_iter().all(|part| {
            part.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_common_secrets_without_logging_them() {
        assert!(contains_sensitive_content("-----BEGIN PRIVATE KEY-----"));
        assert!(contains_sensitive_content("token=sk-example"));
        assert!(contains_sensitive_content(
            "aaaabbbbbb.cccccccccc.dddddddddd"
        ));
        assert!(!contains_sensitive_content(
            "Redis timeout happened after deployment."
        ));
    }

    #[test]
    fn api_key_validation_accepts_realistic_keys() {
        assert!(validate_api_key("sk-1234567890abcdef").is_ok());
        assert!(validate_api_key("  sk-1234567890abcdef  ").is_ok(), "首尾空白应被忽略");
    }

    #[test]
    fn api_key_validation_rejects_obvious_mistakes() {
        assert!(validate_api_key("").is_err(), "空值");
        assert!(validate_api_key("sk-123").is_err(), "太短");
        assert!(validate_api_key("sk-abc def").is_err(), "含空格");
        assert!(validate_api_key("sk-密钥").is_err(), "非 ASCII（复制到中文标点）");
    }

    #[test]
    fn mask_keeps_only_the_tail() {
        let masked = mask_api_key("sk-1234567890abcd");
        assert!(masked.ends_with("abcd"));
        assert!(!masked.contains("1234567890"), "掩码不能泄露中间部分");
    }

    #[test]
    fn status_mapping_separates_user_fixable_failures() {
        assert!(matches!(
            map_status_error(reqwest::StatusCode::UNAUTHORIZED),
            AgentError::Unauthorized
        ));
        assert!(matches!(
            map_status_error(reqwest::StatusCode::PAYMENT_REQUIRED),
            AgentError::InsufficientBalance
        ));
        assert!(matches!(
            map_status_error(reqwest::StatusCode::TOO_MANY_REQUESTS),
            AgentError::RateLimited
        ));
        assert!(matches!(
            map_status_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
            AgentError::ProviderUnavailable
        ));
    }

    #[test]
    fn base_url_accepts_https_and_local_http() {
        assert_eq!(
            normalize_base_url("https://api.deepseek.com").unwrap(),
            "https://api.deepseek.com"
        );
        // 结尾斜杠要吃掉：留着会拼出 //chat/completions
        assert_eq!(
            normalize_base_url("https://api.deepseek.com/").unwrap(),
            "https://api.deepseek.com"
        );
        assert_eq!(
            normalize_base_url("  https://api.openai.com/v1  ").unwrap(),
            "https://api.openai.com/v1"
        );
        // 本地部署是明文 HTTP，这是必须放行的主场景
        assert!(normalize_base_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(normalize_base_url("http://localhost:8000/v1").is_ok());
        assert!(normalize_base_url("http://[::1]:8080/v1").is_ok());
    }

    #[test]
    fn base_url_rejects_plaintext_to_remote_hosts() {
        // 远程明文地址会让 API Key 裸奔，必须拦住
        let err = normalize_base_url("http://api.deepseek.com").unwrap_err();
        assert!(err.contains("https"), "错误信息要告诉用户正确答案：{err}");
        assert!(normalize_base_url("http://192.168.1.50:8000").is_err(), "内网 IP 也不算本机");
        // 伪装成本地的远程地址：主机名以 localhost 开头但不是它
        assert!(normalize_base_url("http://localhost.evil.com").is_err());
    }

    #[test]
    fn base_url_rejects_dangerous_or_malformed_forms() {
        assert!(normalize_base_url("").is_err(), "空值");
        assert!(normalize_base_url("api.deepseek.com").is_err(), "缺协议头");
        assert!(normalize_base_url("https://").is_err(), "只有协议头");
        assert!(normalize_base_url("ftp://example.com").is_err(), "不支持的协议");
        // 凭据不能放在地址里：地址要显示在界面上
        assert!(normalize_base_url("https://user:pass@example.com").is_err());
        assert!(normalize_base_url("https://example.com?key=abc").is_err(), "查询串");
        assert!(normalize_base_url("https://example.com#frag").is_err(), "片段");
    }

    #[test]
    fn provider_label_tells_the_user_where_data_went() {
        assert_eq!(provider_label(DEFAULT_BASE_URL), "DeepSeek");
        // 换成自定义地址后必须显示真实主机名，不能继续报 "DeepSeek"
        assert_eq!(provider_label("https://my-proxy.example.com/v1"), "my-proxy.example.com");
        assert_eq!(provider_label("http://127.0.0.1:11434/v1"), "127.0.0.1");
        assert_eq!(host_of("https://api.deepseek.com"), "api.deepseek.com");
    }

    #[test]
    fn error_codes_are_stable_and_retryable_flags_match() {
        // 前端按 code 分派引导文案，改动这里等于改 API 契约。
        assert_eq!(AgentError::NotConfigured.code(), "ai_not_configured");
        assert_eq!(AgentError::Unauthorized.code(), "ai_unauthorized");
        assert_eq!(AgentError::InsufficientBalance.code(), "ai_insufficient_balance");
        assert_eq!(AgentError::RateLimited.code(), "ai_rate_limited");

        assert!(AgentError::RateLimited.retryable());
        assert!(!AgentError::Unauthorized.retryable(), "Key 无效重试多少次都没用");
        assert!(!AgentError::InsufficientBalance.retryable());
    }
}
