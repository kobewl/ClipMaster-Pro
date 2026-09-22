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

use chrono::{Duration, Utc};
use thiserror::Error;

use crate::application::session_build::{self, SESSION_MAX_ITEMS};
use crate::domain::model::{AgentSessionDetail, AgentSessionSummary};
use crate::domain::ports::{AgentSessionStore, ClipboardRepository};

/// 会话候选的时间窗（天）。
///
/// 量级理由：会话是「最近的工作记忆」，超过几天的内容与当前工作流已经断线；
/// 窗口越宽，重算要扫的候选越多，而工作台又是在打开时同步重算的。
const SESSION_WINDOW_DAYS: i64 = 3;

/// 会话候选的条数上限。
///
/// 与 `session_store::MAX_SESSIONS` 同量级并互相指向：一次重算最多取
/// 这个数的候选，每条会话至少 2 条内容 → 200 / 2 = 100 就是理论上的会话数
/// 上界，store 的裁剪阈值正是按这个数定的。
pub const SESSION_SCAN_LIMIT: u32 = 200;

/// 列表返回的会话条数上限。
///
/// **只保证「最多看到多少条」，不保证「全部都能看到」**：`agent` 行受
/// `session_store::MAX_SESSIONS` 裁剪（100），但 `source='user'` 是用户亲手存的、
/// 永不参与替换与自动裁剪（计划决策 5），**没有上界** —— 用户存到第 101 条时，
/// 较旧的那些在默认列表里就看不见了。数据仍在库里（`detail` 按 id 取得到），
/// 只是工作台目前没有按 id 查询的入口，所以 UI 上不可达。
///
/// 这个「user 会话超过 100 条后不可达」是列表口径的**已知开放问题**
/// （列表口径待定），不是数据丢失。
///
/// 与 `session_store::MAX_SESSIONS` 同值只是让「默认最多看到多少条」在服务层
/// 显式；两者约束的不是同一件事（那边是写入侧的真上界，这边是默认可见窗口）。
pub const SESSION_LIST_LIMIT: u32 = 100;

/// 会话用例的错误。
///
/// 三个码的 `retryable()` **全是 false**：前两条要用户改选择（重试同一个输入
/// 还是同一个结果），第三条是本地库故障（重试无意义，需要用户重启或修环境）。
#[derive(Debug, Error)]
pub enum SessionError {
    /// 条数越界。会话至少 2 条（一条不成组），至多一次批量归纳能处理的上限
    /// （`SESSION_MAX_ITEMS`，见关键判断 8：用户看到超限的会话，
    /// 点「AI 归纳」必然撞 `ai_too_many_items`，那说明分组本身就越界了）。
    #[error("一次会话需要 2 到 {SESSION_MAX_ITEMS} 条内容，当前 {count} 条，请重新选择。")]
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

    /// 重算会话并返回列表 —— 打开工作台的那一次调用。
    ///
    /// 这是「会话只在用户打开工作台时计算」的落地点（关键判断 10）：读接口带
    /// 重算副作用，不单开一个 `rebuild_sessions` 命令（命令面越小越好，那个
    /// 命令也只会被同一个按钮调用）。代价是「读接口有写副作用」，接受的理由是
    /// 它幂等 —— 会话 id 由成员集合派生（关键判断 4），未变化的会话在替换前后
    /// 逐字段相等（`added_at` 是写入时刻，不在幂等口径内），且只写派生表、
    /// 触发者是用户的显式打开动作。
    ///
    /// 流程：取候选（最近 `SESSION_WINDOW_DAYS` 天 ∩ 最近
    /// [`SESSION_SCAN_LIMIT`] 条）→ 纯函数聚类 → **单事务**替换全部
    /// `source='agent'` 会话（用户手动保存的会话永不参与）→ 按
    /// `updated_at DESC` 列前 [`SESSION_LIST_LIMIT`] 条。
    pub async fn refresh_and_list(&self) -> Result<Vec<AgentSessionSummary>, SessionError> {
        let now = Utc::now();
        let since = (now - Duration::days(SESSION_WINDOW_DAYS)).to_rfc3339();
        let candidates = self
            .repository
            .list_since(&since, SESSION_SCAN_LIMIT)
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))?;

        let drafts = session_build::build(&candidates, now);
        self.store
            .replace_agent_sessions(drafts)
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))?;

        self.store
            .list(SESSION_LIST_LIMIT)
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))
    }

    /// 用户显式保存一次会话（多选若干条 → 存为会话）。
    ///
    /// 边界与错误码：
    /// - 去重后条数不在 `SESSION_MIN_ITEMS..=SESSION_MAX_ITEMS` →
    ///   [`SessionError::ItemsInvalid`]（一条不成组，超上限的会话点「AI 归纳」
    ///   必然撞 `ai_too_many_items`，见关键判断 8）；
    /// - 选中的条目已经不在历史里（选择与保存之间被删掉 / 被自动淘汰）→
    ///   [`SessionError::ItemsMissing`]：**如实报出是哪几条**，静默丢掉用户选中
    ///   的人选比报错更糟；
    /// - 用户显式选择**不过**时间 / 来源门槛（那是自动聚类的门槛），
    ///   成员行不带理由 —— 他自己知道为什么把它们放在一起。
    pub async fn save_user_session(
        &self,
        item_ids: Vec<String>,
    ) -> Result<AgentSessionDetail, SessionError> {
        // 去重保序：用户在界面上不可能选中同一条两次，但接口层不能假设调用方
        // 干净 —— 重复 id 会让「条数」与「实际成员数」对不上。
        let mut unique: Vec<String> = Vec::with_capacity(item_ids.len());
        for id in item_ids {
            if !unique.iter().any(|existing| existing == &id) {
                unique.push(id);
            }
        }

        if unique.len() < session_build::SESSION_MIN_ITEMS || unique.len() > SESSION_MAX_ITEMS {
            return Err(SessionError::ItemsInvalid {
                count: unique.len(),
            });
        }

        let found = self
            .repository
            .get_many(&unique)
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))?;
        let missing: Vec<String> = unique
            .into_iter()
            .filter(|id| !found.iter().any(|item| &item.id.to_string() == id))
            .collect();
        if !missing.is_empty() {
            return Err(SessionError::ItemsMissing { missing });
        }

        let draft = session_build::build_user_session(&found, Utc::now());
        let id = draft.id.clone();
        self.store
            .insert_session(draft)
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))?;

        // 回读刚写入的那条：返回库里的真实状态（含 store 赋的成员 `source`
        // 与写入时刻），而不是把草稿原样透出去。查不到说明写入与读取对不上，
        // 那是本地库故障而不是「会话不存在」。
        self.store
            .find(&id)
            .await
            .map_err(|err| SessionError::StoreUnavailable(err.to_string()))?
            .ok_or_else(|| SessionError::StoreUnavailable(format!("会话 {id} 写入后读不回来")))
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
