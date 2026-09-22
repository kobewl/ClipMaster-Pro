//! Flow 会话用例：列表、详情、删除、一键清除。
//!
//! 本文件是**服务骨架**（Task 2）：四个薄封装直接把端口调用转出去，不夹带业务规则。
//! 会话说得清为什么成群（聚类、标题、摘要、逐条 `reason`）由 Task 3 的
//! `session_build` + `refresh_and_list` / `save_user_session` 在此之上叠加。
//!
//! 为什么保留独立的 `SessionError` 而不是复用 `AgentError`：会话是纯本地的派生数据，
//! 不调模型、不碰钥匙串、不发网络请求 —— 塞进 `AgentError` 会把它和「AI 调用失败」
//! 混成同一类，前端只好去猜这一条到底要不要引导用户去设置里配 Key。

use std::sync::Arc;

use thiserror::Error;

use crate::application::agent_service::MAX_AGENT_INPUT_ITEMS;
use crate::domain::model::{AgentSessionDetail, AgentSessionSummary};
use crate::domain::ports::{AgentSessionStore, ClipboardRepository};

/// 会话用例的错误。
///
/// 三个码的 `retryable()` **全是 false**：前两条要用户改选择（重试同一个输入
/// 还是同一个结果），第三条是本地库故障（重试无意义，需要用户重启或修环境）。
#[derive(Debug, Error)]
pub enum SessionError {
    /// 条数越界。会话至少 2 条（一条不成组），至多一次批量归纳能处理的上限
    /// （`MAX_AGENT_INPUT_ITEMS`，见关键判断 8：用户看到超限的会话，
    /// 点「AI 归纳」必然撞 `ai_too_many_items`，那说明分组本身就越界了）。
    #[error("一次会话需要 2 到 {MAX_AGENT_INPUT_ITEMS} 条内容，当前 {count} 条，请重新选择。")]
    ItemsInvalid { count: usize },
    /// 选中的条目已不在历史里（可能在选择与保存之间被删掉或被自动淘汰）。
    #[error("有 {} 条内容已经不在历史里了，请重新选择后保存。", .missing.len())]
    ItemsMissing { missing: Vec<String> },
    /// 本地会话表读不出来或写不进去。
    #[error("无法读写本地会话数据：{0}")]
    StoreUnavailable(String),
}

impl SessionError {
    /// 稳定错误码，前端依赖此字段而非 message 文本。
    pub fn code(&self) -> &'static str {
        match self {
            SessionError::ItemsInvalid { .. } => "ai_session_items_invalid",
            SessionError::ItemsMissing { .. } => "ai_session_item_missing",
            SessionError::StoreUnavailable(_) => "ai_session_store_unavailable",
        }
    }

    /// 能不能引导用户「重试」。
    ///
    /// 逐变体显式给答案（而不是一个 `false` 常量）：将来加第四个变体时，这里会
    /// **编译不过**，逼着做一次「它到底能不能重试」的判断，而不是默默继承一个
    /// 恰好还没被推翻的旧结论。
    pub fn retryable(&self) -> bool {
        match self {
            // 要用户改选择：重试同一个输入还是同一个结果。
            SessionError::ItemsInvalid { .. } => false,
            // 同上：条目已经不在历史里了，再点一次不会把它变回来。
            SessionError::ItemsMissing { .. } => false,
            // 本地库故障：该做的是重启或修环境，不是催用户再点一次。
            SessionError::StoreUnavailable(_) => false,
        }
    }
}

pub struct SessionService {
    /// 会话候选的取数口（`list_since` / `get_many`）。
    ///
    /// Task 2 只接线：`refresh_and_list` / `save_user_session`（Task 3）才真正读它。
    /// 先落在构造里是为了让 Composition Root 只有一处构造点，Task 3 不再动接线 ——
    /// 现在没人读它，这个 `allow` 到 Task 3 接上取数路径时一并删掉。
    #[allow(dead_code)]
    repository: Arc<dyn ClipboardRepository>,
    store: Arc<dyn AgentSessionStore>,
}

impl SessionService {
    pub fn new(
        repository: Arc<dyn ClipboardRepository>,
        store: Arc<dyn AgentSessionStore>,
    ) -> Self {
        Self { repository, store }
    }

    /// 最近 `limit` 条会话，新的在前。
    pub async fn list(&self, limit: u32) -> Result<Vec<AgentSessionSummary>, SessionError> {
        self.store
            .list(limit)
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))
    }

    /// 会话详情。`Ok(None)` = 会话已不在（成员全被删时由触发器带走），
    /// 这是设计的正常结果而不是错误。
    pub async fn detail(&self, id: &str) -> Result<Option<AgentSessionDetail>, SessionError> {
        self.store
            .find(id)
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))
    }

    /// 删一条会话（只删派生数据，原始剪贴板记录一条不动）。返回是否删掉了。
    pub async fn delete(&self, id: &str) -> Result<bool, SessionError> {
        self.store
            .delete(id)
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))
    }

    /// 清空全部会话，返回删掉的条数。
    pub async fn clear(&self) -> Result<u64, SessionError> {
        self.store
            .clear()
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 错误码与重试标志是前端分派文案的依据（照 `agent_service.rs` 的同名钉子）。
    ///
    /// 三个码字面量在 Task 4 才落到命令层，但契约在**这里**就定下了：谁先写代码
    /// 谁就得把字符串钉住，免得实现与计划各写一套。
    #[test]
    fn session_error_codes_and_retryable_flags_are_stable() {
        // 前端按 code 分派引导文案，改动这里等于改 API 契约。
        assert_eq!(
            SessionError::ItemsInvalid { count: 1 }.code(),
            "ai_session_items_invalid"
        );
        assert_eq!(
            SessionError::ItemsMissing {
                missing: vec!["x".to_string()]
            }
            .code(),
            "ai_session_item_missing"
        );
        assert_eq!(
            SessionError::StoreUnavailable("boom".to_string()).code(),
            "ai_session_store_unavailable"
        );

        // 三个变体全不可重试：前两条要用户改输入，第三条是本地库故障。
        // 逐变体断言（而不是一个总的 `!retryable()`）：将来某个变体改成可重试时，
        // 这里会指名道姓地指出是哪一条，而不是一句笼统的「有变体变了」。
        assert!(
            !SessionError::ItemsInvalid { count: 1 }.retryable(),
            "条数越界要用户改选择，重试同一个输入还是同一个结果"
        );
        assert!(
            !SessionError::ItemsMissing {
                missing: vec!["x".to_string()]
            }
            .retryable(),
            "条目已经不在历史里了，再点一次不会把它变回来"
        );
        assert!(
            !SessionError::StoreUnavailable("boom".to_string()).retryable(),
            "本地库故障该做的是重启或修环境，不是催用户再点一次"
        );
    }
}
