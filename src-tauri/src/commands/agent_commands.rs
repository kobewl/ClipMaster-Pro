//! Agent 配置相关命令：读取 / 保存 / 清除凭据与服务地址 / 连通性测试。
//!
//! API Key 只进不出：读取一律返回掩码。

use tauri::State;

use crate::application::agent_service::AgentConfigInfo;
use crate::commands::dto::{SaveAgentEndpointDto, SaveAgentKeyDto};
use crate::domain::error::CommandError;
use crate::lifecycle::runtime::AppRuntime;

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

fn agent_error(err: crate::application::agent_service::AgentError) -> CommandError {
    CommandError::new(err.code(), err.to_string(), err.retryable())
}
