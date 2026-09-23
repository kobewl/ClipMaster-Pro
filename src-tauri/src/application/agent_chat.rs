//! 对话 Agent：自然语言 + 只读工具循环 + 流式输出。
//!
//! 和五按钮动作共用 [`AgentService`] 的闸门 / 冷却 / 取消 / 审计。
//! 对话正文落在派生表里（可一键清除），审计仍然只记元数据。

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::application::agent_service::{
    contains_sensitive_content, map_status_error, provider_label, AgentError, AgentService,
    SharedGateAudit,
};
use crate::application::agent_tools;
use crate::domain::error::RepositoryError;
use crate::domain::model::{AgentChatDetail, AgentChatMessage, AgentChatSummary};
use crate::domain::ports::AgentChatStore;

const SYSTEM_PROMPT: &str = "\
你是 ClipMaster 的本地剪贴板助手。用户的剪贴板历史在本机，你通过工具搜索和阅读，不要编造条目。
规则：
- 用户提到某段内容、某次排查、昨天复制的东西时，先 search_clipboard，再 read_items。
- 引用读到的内容时用 [编号]，编号与 read_items 返回的 index 一致。
- 被标记 blocked_sensitive 的条目不要猜测其内容。
- 用用户使用的语言回答；默认简体中文。
- 你现在不能复制、粘贴或改分组。需要用户动手时，明确告诉他点界面上的对应按钮。
- 不要输出工具的原始 JSON。";

const MAX_TOOL_ROUNDS: usize = 4;
const MAX_HISTORY_MESSAGES: usize = 16;
const MAX_ATTACHED_ITEMS: usize = 5;
const MAX_USER_CHARS: usize = 4_000;
const SSE_BUFFER_CAP: usize = 256 * 1024;

pub struct AgentChatService {
    agent: Arc<AgentService>,
    store: Arc<dyn AgentChatStore>,
}

#[derive(Debug, Clone)]
pub struct ChatSendRequest {
    pub conversation_id: Option<String>,
    pub message: String,
    pub item_ids: Vec<String>,
    pub search_hint: Option<String>,
    pub request_id: Option<String>,
}

impl AgentChatService {
    pub fn new(agent: Arc<AgentService>, store: Arc<dyn AgentChatStore>) -> Self {
        Self { agent, store }
    }

    pub async fn list(&self, limit: u32) -> Result<Vec<AgentChatSummary>, AgentError> {
        self.store.list(limit.min(30)).await.map_err(store_err)
    }

    pub async fn get(&self, id: &str) -> Result<Option<AgentChatDetail>, AgentError> {
        self.store.find(id).await.map_err(store_err)
    }

    pub async fn delete(&self, id: &str) -> Result<bool, AgentError> {
        self.store.delete(id).await.map_err(store_err)
    }

    pub async fn clear(&self) -> Result<u64, AgentError> {
        self.store.clear().await.map_err(store_err)
    }

    pub async fn send(
        &self,
        request: ChatSendRequest,
        app: AppHandle,
    ) -> Result<AgentChatDetail, AgentError> {
        let message = request.message.trim();
        if message.is_empty() {
            return Err(AgentError::EmptyMessage);
        }
        if message.chars().count() > MAX_USER_CHARS {
            return Err(AgentError::InputTooLong);
        }
        if contains_sensitive_content(message) {
            return Err(AgentError::SensitivePrompt);
        }

        let item_ids: Vec<String> = request
            .item_ids
            .into_iter()
            .filter(|id| !id.trim().is_empty())
            .take(MAX_ATTACHED_ITEMS)
            .collect();
        let search_hint = request
            .search_hint
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let now = chrono::Utc::now().to_rfc3339();
        let (chat_id, history) = if let Some(id) = request.conversation_id.as_deref() {
            let existing = self
                .store
                .find(id)
                .await
                .map_err(store_err)?
                .ok_or(AgentError::ChatUnavailable)?;
            (existing.id, existing.messages)
        } else {
            let id = format!("chat-{}", Uuid::new_v4());
            let title = title_from(message);
            self.store
                .insert(&id, &title, &now, &now)
                .await
                .map_err(store_err)?;
            (id, Vec::new())
        };

        self.store
            .append_message(&chat_id, "user", message, &now)
            .await
            .map_err(store_err)?;
        self.store
            .touch(&chat_id, None, &now)
            .await
            .map_err(store_err)?;

        let emit_id = request
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let user_text = compose_user_message(message, &item_ids, search_hint.as_deref());
        let app_for_loop = app.clone();
        let agent = self.agent.clone();
        let cited = item_ids.clone();
        let history_for_loop = history;
        let emit_id_for_loop = emit_id.clone();
        let _ = app.emit(
            "agent-chat://started",
            json!({
                "request_id": emit_id,
                "conversation_id": chat_id,
            }),
        );

        let reply = self
            .agent
            .with_shared_gate(request.request_id.clone(), "chat", item_ids, move || {
                let agent = agent.clone();
                let app = app_for_loop.clone();
                let history = history_for_loop;
                let user_text = user_text.clone();
                let cited = cited.clone();
                let emit_id = emit_id_for_loop;
                async move {
                    run_tool_loop(&agent, &app, &emit_id, &history, &user_text, &cited).await
                }
            })
            .await?;

        let done_at = chrono::Utc::now().to_rfc3339();
        self.store
            .append_message(&chat_id, "assistant", &reply, &done_at)
            .await
            .map_err(store_err)?;
        self.store
            .touch(&chat_id, None, &done_at)
            .await
            .map_err(store_err)?;
        self.store
            .find(&chat_id)
            .await
            .map_err(store_err)?
            .ok_or(AgentError::ChatUnavailable)
    }
}

fn store_err(err: RepositoryError) -> AgentError {
    AgentError::RunStoreUnavailable(err.to_string())
}

fn title_from(message: &str) -> String {
    let flat = message.trim().replace('\n', " ");
    let chars: Vec<char> = flat.chars().collect();
    if chars.len() <= 36 {
        flat
    } else {
        format!("{}…", chars.into_iter().take(36).collect::<String>())
    }
}

fn compose_user_message(message: &str, item_ids: &[String], search_hint: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(hint) = search_hint.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("当前搜索框：{hint}"));
    }
    if !item_ids.is_empty() {
        parts.push(format!(
            "用户附带了 {} 条剪贴板记录，id：{}",
            item_ids.len(),
            item_ids.join("、")
        ));
        parts.push("请先 read_items 读这些 id，再回答。".into());
    }
    if parts.is_empty() {
        return message.to_string();
    }
    format!("{}\n\n{}", parts.join("\n"), message)
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<StreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<StreamFunction>,
}

#[derive(Debug, Deserialize)]
struct StreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Option<Vec<StreamChoice>>,
}

#[derive(Default)]
struct ToolAccumulator {
    id: String,
    name: String,
    arguments: String,
}

async fn run_tool_loop(
    agent: &AgentService,
    app: &AppHandle,
    request_id: &str,
    history: &[AgentChatMessage],
    user_text: &str,
    cited: &[String],
) -> Result<(String, SharedGateAudit), AgentError> {
    let config = agent.config().await?;
    config.ensure_key_not_sent_astray()?;
    let provider = provider_label(&config.base_url);
    let api_key = config.api_key.ok_or(AgentError::NotConfigured)?;

    let mut messages = vec![json!({"role": "system", "content": SYSTEM_PROMPT})];
    let start = history.len().saturating_sub(MAX_HISTORY_MESSAGES);
    for item in &history[start..] {
        if item.role == "user" || item.role == "assistant" {
            messages.push(json!({"role": item.role, "content": item.content}));
        }
    }
    messages.push(json!({"role": "user", "content": user_text}));

    if !cited.is_empty() {
        if let Ok(prefetched) = agent_tools::execute(
            agent.history(),
            "read_items",
            &json!({ "item_ids": cited }).to_string(),
        )
        .await
        {
            messages.push(json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "prefetch_read",
                    "type": "function",
                    "function": { "name": "read_items", "arguments": json!({ "item_ids": cited }).to_string() }
                }]
            }));
            messages.push(json!({
                "role": "tool",
                "tool_call_id": "prefetch_read",
                "content": prefetched
            }));
        }
    }

    let mut input_chars = user_text.chars().count() as u64;
    let mut last_text = String::new();
    agent.mark_request_started();

    for _round in 0..MAX_TOOL_ROUNDS {
        let (text, tools) = stream_completion(
            agent,
            app,
            request_id,
            &config.base_url,
            &config.model,
            &api_key,
            &provider,
            &messages,
        )
        .await?;
        if tools.is_empty() {
            last_text = text;
            break;
        }
        messages.push(json!({
            "role": "assistant",
            "content": if text.is_empty() { Value::Null } else { Value::String(text) },
            "tool_calls": tools.iter().map(|call| json!({
                "id": call.id,
                "type": "function",
                "function": { "name": call.name, "arguments": call.arguments }
            })).collect::<Vec<_>>()
        }));
        for call in tools {
            let _ = app.emit(
                "agent-chat://tool",
                json!({
                    "request_id": request_id,
                    "name": call.name,
                    "phase": "start",
                    "detail": tool_detail(&call.name, &call.arguments),
                }),
            );
            tracing::info!(tool = %call.name, "对话 Agent 调用只读工具");
            let result = agent_tools::execute(agent.history(), &call.name, &call.arguments)
                .await
                .unwrap_or_else(|err| json!({ "error": err }).to_string());
            input_chars += result.chars().count() as u64;
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call.id,
                "content": result
            }));
            let _ = app.emit(
                "agent-chat://tool",
                json!({
                    "request_id": request_id,
                    "name": call.name,
                    "phase": "done",
                    "detail": "",
                }),
            );
        }
    }

    if last_text.is_empty() {
        last_text = "我查过了，但这一轮没有形成可用的回答。请换个说法再问一次。".into();
    }

    Ok((
        last_text.clone(),
        SharedGateAudit {
            input_chars,
            output_chars: last_text.chars().count() as u64,
            input_item_ids: cited.to_vec(),
            target: Some((provider, config.model)),
        },
    ))
}

fn tool_detail(name: &str, arguments: &str) -> String {
    if name != "search_clipboard" {
        return String::new();
    }
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| value.get("query")?.as_str().map(str::to_string))
        .unwrap_or_default()
}

async fn stream_completion(
    agent: &AgentService,
    app: &AppHandle,
    request_id: &str,
    base_url: &str,
    model: &str,
    api_key: &str,
    provider: &str,
    messages: &[Value],
) -> Result<(String, Vec<ToolAccumulator>), AgentError> {
    let response = agent
        .http()
        .post(format!("{base_url}/chat/completions"))
        .timeout(std::time::Duration::from_secs(90))
        .bearer_auth(api_key)
        .json(&json!({
            "model": model,
            "temperature": 0.2,
            "stream": true,
            "messages": messages,
            "tools": agent_tools::tool_schemas(),
        }))
        .send()
        .await
        .map_err(|_| AgentError::ProviderUnavailable {
            provider: provider.to_string(),
        })?;

    if !response.status().is_success() {
        let status = response.status();
        tracing::warn!(status = %status, "对话模型请求失败");
        return Err(map_status_error(status, provider));
    }

    let mut content = String::new();
    let mut tools: Vec<ToolAccumulator> = Vec::new();
    let mut buffer = String::new();
    let mut stream = response;
    while let Some(chunk) = stream
        .chunk()
        .await
        .map_err(|_| AgentError::ProviderUnavailable {
            provider: provider.to_string(),
        })?
    {
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        if buffer.len() > SSE_BUFFER_CAP {
            return Err(AgentError::InvalidResponse {
                provider: provider.to_string(),
            });
        }
        while let Some(index) = buffer.find('\n') {
            let line = buffer[..index].trim_end_matches('\r').to_string();
            buffer = buffer[index + 1..].to_string();
            consume_sse_line(&line, request_id, &mut content, &mut tools, app);
        }
    }
    if !buffer.is_empty() {
        consume_sse_line(
            buffer.trim_end_matches('\r'),
            request_id,
            &mut content,
            &mut tools,
            app,
        );
    }
    Ok((
        content,
        tools
            .into_iter()
            .filter(|call| !call.name.is_empty())
            .collect(),
    ))
}

fn consume_sse_line(
    line: &str,
    request_id: &str,
    content: &mut String,
    tools: &mut Vec<ToolAccumulator>,
    app: &AppHandle,
) {
    let Some(data) = line.strip_prefix("data:") else {
        return;
    };
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return;
    }
    let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) else {
        return;
    };
    let Some(choice) = chunk.choices.and_then(|choices| choices.into_iter().next()) else {
        return;
    };
    let Some(delta) = choice.delta else {
        return;
    };
    if let Some(text) = delta.content {
        if !text.is_empty() {
            content.push_str(&text);
            let _ = app.emit(
                "agent-chat://delta",
                json!({ "request_id": request_id, "text": text }),
            );
        }
    }
    if let Some(calls) = delta.tool_calls {
        for call in calls {
            while tools.len() <= call.index {
                tools.push(ToolAccumulator::default());
            }
            let slot = &mut tools[call.index];
            if let Some(id) = call.id {
                slot.id = id;
            }
            if let Some(function) = call.function {
                if let Some(name) = function.name {
                    slot.name = name;
                }
                if let Some(arguments) = function.arguments {
                    slot.arguments.push_str(&arguments);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_from_keeps_short_text_and_clips_long_ones() {
        assert_eq!(title_from("  昨天的 redis 命令  "), "昨天的 redis 命令");
        let long = "字".repeat(40);
        let title = title_from(&long);
        assert!(title.ends_with('…'), "{title}");
        assert_eq!(title.chars().count(), 37);
    }

    #[test]
    fn compose_user_message_only_adds_context_when_present() {
        assert_eq!(compose_user_message("你好", &[], None), "你好");
        let with_search = compose_user_message("帮我找", &[], Some(" redis "));
        assert!(with_search.contains("当前搜索框：redis"), "{with_search}");
        assert!(with_search.contains("帮我找"), "{with_search}");

        let with_items = compose_user_message("这是什么", &["a".into(), "b".into()], None);
        assert!(with_items.contains("2 条"), "{with_items}");
        assert!(with_items.contains("a、b"), "{with_items}");
        assert!(with_items.contains("read_items"), "{with_items}");
    }

    #[test]
    fn tool_detail_only_exposes_search_query() {
        assert_eq!(tool_detail("read_items", r#"{"item_ids":["x"]}"#), "");
        assert_eq!(
            tool_detail("search_clipboard", r#"{"query":"redis 超时"}"#),
            "redis 超时"
        );
    }
}
