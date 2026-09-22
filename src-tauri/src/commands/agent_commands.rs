//! Agent 配置相关命令：读取 / 保存 / 清除凭据与服务地址 / 连通性测试 / 使用记录。
//!
//! API Key 只进不出：读取一律返回掩码。
//!
//! Flow 会话的五个命令也在本文件（`list_agent_sessions` … `clear_agent_derived_data`）：
//! 会话是 AI 派生数据的一部分，与使用记录同属一个命令命名空间 ——
//! 任务书写的是 `list_sessions`，落地统一带 `agent_` 前缀，与既有
//! `list_agent_runs` / `get_agent_run` 一致。

use tauri::State;

use crate::application::agent_service::{AgentAction, AgentConfigInfo};
use crate::application::session_service::SessionError;
use crate::commands::dto::{
    AgentRunDto, AgentSessionDetailDto, AgentSessionSummaryDto, ClearDerivedDataDto,
    CreateAgentSessionDto, SaveAgentEndpointDto, SaveAgentKeyDto,
};
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
///
/// **必须带号**：只取消号匹配的那个请求，号对不上返回 false 且不打断任何东西。
/// 这样即便将来多个请求并存，用户按的「取消」也只会落在自己发起的那一次上。
#[tauri::command]
pub async fn cancel_agent_action(
    runtime: State<'_, AppRuntime>,
    request_id: String,
) -> Result<bool, CommandError> {
    Ok(runtime.agent.cancel(&request_id))
}

// ---------------------------------------------------------------------------
//  Flow 会话（Phase 1 第 3 步）
// ---------------------------------------------------------------------------

/// 会话列表，新的在前 —— 打开工作台的那一次调用。
///
/// **带重算副作用**（关键判断 10）：先重算再列返回，这是「会话只在用户打开
/// 工作台时计算」的落地点（不做后台轮询）。不单开 `rebuild_sessions` 命令：
/// 命令面越小越好，那个命令也只会被同一个按钮调用。副作用是幂等的
/// （会话 id 由成员集合派生），且只写派生表。
#[tauri::command]
pub async fn list_agent_sessions(
    runtime: State<'_, AppRuntime>,
) -> Result<Vec<AgentSessionSummaryDto>, CommandError> {
    let sessions = runtime
        .sessions
        .refresh_and_list()
        .await
        .map_err(session_error)?;
    Ok(sessions
        .into_iter()
        .map(AgentSessionSummaryDto::from)
        .collect())
}

/// 会话详情（成员按 `position` 升序，编号就是界面上的编号）。
///
/// 返回 `None` 而不是报错：会话可能已经随它最后一条成员一起被删掉了
/// （触发器带走，见 migration 7）—— 那是设计的正常结果，
/// 不该让界面显示一个错误（照 `get_agent_run` 的先例）。
#[tauri::command]
pub async fn get_agent_session(
    runtime: State<'_, AppRuntime>,
    id: String,
) -> Result<Option<AgentSessionDetailDto>, CommandError> {
    let session = runtime.sessions.detail(&id).await.map_err(session_error)?;
    Ok(session.map(AgentSessionDetailDto::from))
}

/// 用户显式保存一次会话（多选若干条 → 「存为会话」）。
///
/// 与打开工作台不同，这条命令**不重算**：用户亲手选的成员就是他要的结果，
/// 让重算去覆盖等于把他刚做的操作删掉（重算只替换 `source='agent'`）。
#[tauri::command]
pub async fn create_agent_session(
    runtime: State<'_, AppRuntime>,
    request: CreateAgentSessionDto,
) -> Result<AgentSessionDetailDto, CommandError> {
    let session = runtime
        .sessions
        .save_user_session(request.item_ids)
        .await
        .map_err(session_error)?;
    Ok(AgentSessionDetailDto::from(session))
}

/// 删一条会话，返回是否真的删掉了。
///
/// 只删派生数据：原始剪贴板记录一条不动。`false` 不是错误 ——
/// 会话可能已经被删过一次（或随最后一条成员消失），结果与用户想要的相同。
#[tauri::command]
pub async fn delete_agent_session(
    runtime: State<'_, AppRuntime>,
    id: String,
) -> Result<bool, CommandError> {
    runtime.sessions.delete(&id).await.map_err(session_error)
}

/// 一键清除所有 AI 派生数据（会话 + 使用记录），返回两边各自删掉的条数。
///
/// 顺序固定为**先会话、后审计**，各自走自己的事务（关键判断 11）：这是用户
/// 手动触发的幂等操作，不做跨表事务 —— 失败就如实报错、让他重试（重试无害），
/// 为一个「删派生数据」的按钮引入跨 store 事务协调不值得。原始剪贴板记录不受影响。
#[tauri::command]
pub async fn clear_agent_derived_data(
    runtime: State<'_, AppRuntime>,
) -> Result<ClearDerivedDataDto, CommandError> {
    let sessions = runtime.sessions.clear().await.map_err(session_error)?;
    let runs = runtime.agent.clear_runs().await.map_err(agent_error)?;
    Ok(ClearDerivedDataDto { sessions, runs })
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

/// 会话用例的错误映射。
///
/// 与 [`agent_error`] 分开的理由同 `SessionError` 自己存在的理由：会话是纯本地的
/// 派生数据，前端拿到的 `ai_session_*` 三个码不会把它误读成「AI 调用失败」而
/// 把用户往「设置 → AI 助手」引。code 与 retryable 全部由 `SessionError`
/// 自己给出，命令层不重写一个 `false`（契约的真相源只有服务层那一处）。
fn session_error(err: SessionError) -> CommandError {
    CommandError::new(err.code(), err.to_string(), err.retryable())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 命令层是错误码**离开进程前**的最后一站：这里必须逐字透传服务层的 code
    /// 与 retryable，不许改写。`session_error` 是纯函数，所以这条钉子不需要
    /// `AppRuntime`（那东西要 Tauri `AppHandle` 与采集管线，测试构造不出来）——
    /// 五个命令本身仍只有前端 mock invoke 与人工跑一遍覆盖。
    #[test]
    fn session_error_passes_the_service_code_through_unchanged() {
        for (err, expected_code) in [
            (
                SessionError::ItemsInvalid { count: 1 },
                "ai_session_items_invalid",
            ),
            (
                SessionError::ItemsMissing {
                    missing: vec!["x".to_string()],
                },
                "ai_session_item_missing",
            ),
            (
                SessionError::StoreUnavailable("boom".to_string()),
                "ai_session_store_unavailable",
            ),
        ] {
            let mapped = session_error(err);
            assert_eq!(mapped.code, expected_code);
            assert!(!mapped.retryable, "三个会话错误码都不可重试");
            assert!(
                !mapped.message.is_empty(),
                "message 必须给用户一句可读的提示"
            );
        }
    }
}
