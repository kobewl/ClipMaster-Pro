//! Agent 配置相关命令：读取 / 保存 / 清除凭据与服务地址 / 连通性测试 / 使用记录。
//!
//! API Key 只进不出：读取一律返回掩码。

use tauri::State;

use crate::application::agent_service::{AgentAction, AgentConfigInfo};
use crate::commands::dto::{AgentRunDto, SaveAgentEndpointDto, SaveAgentKeyDto};
use crate::domain::error::CommandError;
use crate::domain::ports::AgentRunRecord;
use crate::lifecycle::runtime::AppRuntime;

/// 一次返回多少条使用记录。审计是"最近发生了什么"，不是归档。
const RECENT_RUNS_LIMIT: u32 = 50;

#[tauri::command]
pub async fn get_agent_config(
    runtime: State<'_, AppRuntime>,
) -> Result<AgentConfigInfo, CommandError> {
    runtime.agent.config_info().await.map_err(agent_error)
}

#[tauri::command]
pub async fn save_agent_key(
    runtime: State<'_, AppRuntime>,
    request: SaveAgentKeyDto,
) -> Result<AgentConfigInfo, CommandError> {
    runtime
        .agent
        .save_api_key(&request.api_key)
        .await
        .map_err(agent_error)
}

#[tauri::command]
pub async fn clear_agent_key(
    runtime: State<'_, AppRuntime>,
) -> Result<AgentConfigInfo, CommandError> {
    runtime.agent.clear_api_key().await.map_err(agent_error)
}

/// 保存自定义服务地址与模型名。
#[tauri::command]
pub async fn save_agent_endpoint(
    runtime: State<'_, AppRuntime>,
    request: SaveAgentEndpointDto,
) -> Result<AgentConfigInfo, CommandError> {
    runtime
        .agent
        .save_endpoint(&request.base_url, &request.model)
        .await
        .map_err(agent_error)
}

/// 恢复内置默认地址与模型。
#[tauri::command]
pub async fn reset_agent_endpoint(
    runtime: State<'_, AppRuntime>,
) -> Result<AgentConfigInfo, CommandError> {
    runtime.agent.reset_endpoint().await.map_err(agent_error)
}

/// 测试连接：只调 `/models`，不产生推理费用。
#[tauri::command]
pub async fn test_agent_connection(runtime: State<'_, AppRuntime>) -> Result<(), CommandError> {
    runtime.agent.test_connection().await.map_err(agent_error)
}

/// 最近的 AI 使用记录，新的在前。
#[tauri::command]
pub async fn list_agent_runs(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<AgentRunDto>, CommandError> {
    let runs = runtime
        .agent
        .recent_runs(RECENT_RUNS_LIMIT)
        .await
        .map_err(agent_error)?;
    Ok(runs.into_iter().map(AgentRunDto::from).collect())
}

/// 清空使用记录，返回删掉的条数。
#[tauri::command]
pub async fn clear_agent_runs(runtime: State<'_, AppRuntime>) -> Result<u64, CommandError> {
    runtime.agent.clear_runs().await.map_err(agent_error)
}

/// 取一条使用记录（结果卡片的「查看来源」）。
///
/// 返回 `None` 而不是报错：记录可能已经随原始条目一起被删掉了 ——
/// 那是设计的正常结果，不该让界面显示一个错误。
#[tauri::command]
pub async fn get_agent_run(
    runtime: State<'_, AppRuntime>,
    id: String,
) -> Result<Option<AgentRunDto>, CommandError> {
    let run = runtime.agent.find_run(&id).await.map_err(agent_error)?;
    Ok(run.map(AgentRunDto::from))
}

/// 取消进行中的 AI 请求；返回是否确有请求被取消。
#[tauri::command]
pub async fn cancel_agent_action(
    runtime: State<'_, AppRuntime>,
) -> Result<bool, CommandError> {
    Ok(runtime.agent.cancel())
}

impl From<AgentRunRecord> for AgentRunDto {
    fn from(record: AgentRunRecord) -> Self {
        // 认不出来的动作名原样显示：将来加了新动作，旧版本的界面
        // 至少还能显示一个可读的标识，而不是空白。
        let action_label = AgentAction::from_key(&record.action)
            .map(|action| action.label().to_string())
            .unwrap_or_else(|| record.action.clone());
        Self {
            id: record.id,
            created_at: record.created_at,
            action: record.action,
            action_label,
            provider: record.provider,
            model: record.model,
            input_item_ids: record.input_item_ids,
            input_chars: record.input_chars,
            status: record.status,
            error_code: record.error_code,
            duration_ms: record.duration_ms,
            output_chars: record.output_chars,
        }
    }
}

fn agent_error(err: crate::application::agent_service::AgentError) -> CommandError {
    CommandError::new(err.code(), err.to_string(), err.retryable())
}
