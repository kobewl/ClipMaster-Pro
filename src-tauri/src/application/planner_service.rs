//! Planner 建议用例：在一条会话上产出「下一步做什么」的建议（Phase 1 第 5 步）。
//!
//! 六步编排：① 读会话详情 → ② 取成员 id（`position` 升序）→ ③ 调模型
//! （走 `AgentService::run_planner_with_request_id`，与四类动作共用闸门 / 冷却 /
//! 取消 / 审计）→ ④ 解析模型输出（`planner_build`，白名单是唯一入口）→
//! ⑤ **重新读一次会话详情**，把编号映射成 `PlannerTarget` → ⑥ 组装成
//! `PlannerSuggestionSet`。
//!
//! 两个刻意的设计点：
//!
//! 1. **建议不落库**：返回值就是这次建议的全部，不写表、不写文件、不进审计正文
//!    （审计只多一行元数据：`action = "suggest_actions"`、输入是本次会话成员）。
//!    存建议等于存模型响应正文，而审计的隐私边界明写了「不存 prompt 正文与
//!    模型响应正文」；建议的时效性也强（依赖会话当下成员），存下来第二天再显示
//!    反而是坏体验。
//! 2. **模型不能指定任意条目**：`planner_build::parse` 先把编号限定在本次真正
//!    送出的 `[n]` 上，这里再对着**调用之后**的会话成员核对一遍 —— 模型调用
//!    期间可能有成员被删（用户删条目、按天清理、按条数淘汰），那时指向它的建议
//!    必须整条丢弃，而不是拿着一个已经不在会话里的条目去让用户确认。
//!
//! 为什么保留独立的 `PlannerError`：两个新码（`planner_session_missing` /
//! `planner_suggestions_unusable`）都不是「Key / 设置」类问题，用 `ai_*` 前缀
//! 会把用户往「设置 → AI 助手」引；其余错误一律**透传**既有码，一个都不重写。

use std::sync::Arc;

use thiserror::Error;

use crate::application::agent_service::{AgentError, AgentService};
use crate::application::planner_build;
use crate::application::session_service::SessionError;
use crate::domain::model::{ClipboardItem, PlannerSuggestion, PlannerSuggestionSet, PlannerTarget};
use crate::domain::ports::AgentSessionStore;

/// 会话 store 故障 → 透传 `ai_session_store_unavailable`。
///
/// 抽成自由函数而不是在每处写一遍闭包：两处调用（读会话、调用后重读）必须是
/// **同一个**错误口径，写成两份迟早分叉。
fn store_error(err: crate::domain::error::RepositoryError) -> PlannerError {
    PlannerError::Session(SessionError::StoreUnavailable(err.to_string()))
}

/// 会话相关的错误（store 故障透传 `ai_session_*`）。
#[derive(Debug, Error)]
pub enum PlannerError {
    /// 会话已经不在了（成员被删到一条不剩时由触发器带走）。
    ///
    /// **成员为空的畸形状态也折叠进这一条**：孤儿触发器保证空会话不存在，
    /// 为一条不可达状态单开一个码只会增加前端的分派面。
    #[error("这次会话已经不在了，可能其中的记录都被删掉了。")]
    SessionGone,
    /// 模型整段输出里**一行都没活下来**，而且它也没说「没有建议」。
    ///
    /// 与 `ai_invalid_response` 同一类：模型返回了不符合协议的内容，再试一次
    /// 可能成功 —— 所以 `retryable()` 是 `true`。`dropped` 是丢弃行数，如实告知
    /// 「你给的 N 行都不合规」比一句「解析失败」有用。
    ///
    /// **已知口径重叠（本轮登记，不修）**：`dropped` 同时计入了两类来源 ——
    /// ① `planner_build::parse` 丢掉的不合规行（模型的问题）；② 第 ⑤ 步调用后
    /// 重校验时「目标已经不在会话里」的整条丢弃（**用户的问题**：他在模型响应
    /// 在途时删了成员）。当 `dropped` 全部来自 ② 时，本变体的 message「模型返回的
    /// 建议不符合协议」归因不准（模型其实完全合规），而 `retryable = true` 还会
    /// 鼓励一次没有意义的重复调用。本期接受的理由：②出现的窗口很窄（只有调用在途
    /// 那几秒里删成员），且此时返回一个残缺建议集比报错更危险；把两类来源在文案层
    /// 分开需要多一个错误变体与一条前端分支，留给 Task 4 的文案层处理。**本变体的
    /// code 与 message 不动**（错误码只增不改）。
    #[error("模型返回的建议不符合协议（{dropped} 行被忽略），请稍后重试。")]
    SuggestionsUnusable { dropped: usize },
    /// 会话 store 故障。`ai_session_*` 原样转出，命令层不重写。
    #[error("{0}")]
    Session(#[from] SessionError),
    /// 模型调用失败。`ai_*` 原样转出（含 `ai_busy` / `ai_cooldown` /
    /// `ai_not_configured` / `ai_sensitive_content` / `ai_too_many_items`）——
    /// 前端的语境化改写只在显示层，码与文案一个字不改。
    #[error("{0}")]
    Agent(#[from] AgentError),
}

impl PlannerError {
    /// 稳定错误码，前端依赖此字段而非 message 文本。
    ///
    /// 只新增两个 `planner_` 码；其余一律透传既有码（命令层也不重写），
    /// 所以「错误码只增不改」这条在 Planner 链路上同样成立。
    pub fn code(&self) -> &'static str {
        match self {
            PlannerError::SessionGone => "planner_session_missing",
            PlannerError::SuggestionsUnusable { .. } => "planner_suggestions_unusable",
            PlannerError::Session(err) => err.code(),
            PlannerError::Agent(err) => err.code(),
        }
    }

    /// 能不能引导用户「重试」。
    ///
    /// 逐变体显式给答案（而不是一个 `false` 常量）：将来加第五个变体时，这里会
    /// **编译不过**，逼着做一次判断，而不是默默继承一个恰好还没被推翻的旧结论。
    pub fn retryable(&self) -> bool {
        match self {
            // 会话不在了：重试同一个号还是同一个结果。
            PlannerError::SessionGone => false,
            // 模型没按协议输出：再试一次可能成功（照 ai_invalid_response 的口径）。
            PlannerError::SuggestionsUnusable { .. } => true,
            PlannerError::Session(err) => err.retryable(),
            PlannerError::Agent(err) => err.retryable(),
        }
    }
}

pub struct PlannerService {
    store: Arc<dyn AgentSessionStore>,
    agent: Arc<AgentService>,
}

impl PlannerService {
    pub fn new(store: Arc<dyn AgentSessionStore>, agent: Arc<AgentService>) -> Self {
        Self { store, agent }
    }

    /// 在一条会话上产出建议。
    ///
    /// `request_id` 由前端预置（理由同 `run_with_request_id`：界面要能按号取消、
    /// 迟到响应要能做守卫），并成为审计行的主键。
    ///
    /// `Ok(空集)` 只有一种来源：模型明确说了「没有值得执行的下一步」。一行都没
    /// 活下来且模型没说 `NONE` 时返回的是 [`PlannerError::SuggestionsUnusable`]，
    /// 所以调用方不需要再区分两种「空」。
    pub async fn suggest(
        &self,
        session_id: &str,
        request_id: Option<String>,
    ) -> Result<PlannerSuggestionSet, PlannerError> {
        // ① 读会话详情。`None` = 会话已不在（成员被删到一条不剩时由触发器带走）。
        let session = self.store.find(session_id).await.map_err(store_error)?;
        let session = session.ok_or(PlannerError::SessionGone)?;

        // ② 成员 id，按 `position` 升序（store 已保证顺序）。
        // 空集合折叠成 SessionGone：孤儿触发器保证空会话不存在，走到这里说明库
        // 被人改过，此时「会话不在了」比「模型返回空建议」更接近事实。
        let member_ids: Vec<String> = session
            .members
            .iter()
            .map(|member| member.item.id.to_string())
            .collect();
        if member_ids.is_empty() {
            return Err(PlannerError::SessionGone);
        }

        // ③ 调模型。错误（含 ai_busy / ai_cooldown / ai_sensitive_content）透传；
        // 本地门禁在这一步之内就会拦住敏感内容，一个字都发不出去。
        let result = self
            .agent
            .run_planner_with_request_id(&member_ids, request_id)
            .await?;

        // ④ 解析。`sent` 是「prompt 里实际送出的 (编号, item_id)」—— 与模型看到的
        // `[n]` 一一对应，不是会话的 position（prompt 按「最近优先」重排过）。
        let sent: Vec<(usize, String)> = result
            .inputs
            .iter()
            .map(|input| (input.index, input.item_id.clone()))
            .collect();
        let plan = planner_build::parse(&result.content, &sent);

        // ⑤ 重新读一次会话详情：模型调用期间可能有成员被删 / 会话被触发器带走。
        // 拿**当下**的成员做映射，指向已不在会话里的条目就映射不到，整条丢弃。
        // 会话整体消失时不再假装「模型说了没有建议」，而是如实报会话已不在。
        let current = self.store.find(session_id).await.map_err(store_error)?;
        let current = current.ok_or(PlannerError::SessionGone)?;
        // 键是条目 id 的字符串形态；`HashMap<String, _>` 用 `get(&str)` 查得中
        // （`String: Borrow<str>`），所以解析出来的 id 不用再分配一次。
        let mut by_item_id: std::collections::HashMap<String, (&ClipboardItem, i64)> =
            std::collections::HashMap::with_capacity(current.members.len());
        for member in &current.members {
            by_item_id.insert(member.item.id.to_string(), (&member.item, member.position));
        }

        let mut dropped = plan.dropped;
        let mut suggestions: Vec<PlannerSuggestion> = Vec::new();
        for parsed in plan.items {
            // M-3：目标去重保序。模型把同一个编号写多遍是常见手滑（`[2,2,3]`），
            // 接口层不能假设调用方干净 —— 照 `session_service::save_user_session`
            // 的先例（那边注释写了同一条理由）。重复 ID 会让「目标数」与实际
            // 目标数对不上，确认卡片上就会出现同一个条目两遍。
            let mut targets: Vec<PlannerTarget> = Vec::with_capacity(parsed.target_item_ids.len());
            let mut resolved = true;
            for item_id in &parsed.target_item_ids {
                let Some((item, position)) = by_item_id.get(item_id.as_str()) else {
                    // 这条建议指向了一个已经不在会话里的条目：**整条丢弃**（不猜、
                    // 不换成别的条目、也不保留映射得上的那部分）。半截建议比没有
                    // 建议更危险 —— 确认卡片上显示的目标与模型真正想指的不是一回事，
                    // 用户按着一个被裁剪过的目标去确认，副作用就落在一个模型从未
                    // 指定过的集合上。判别性用例：`planner_tests.rs` 的
                    // `planner_drops_a_multi_target_suggestion_when_one_target_is_gone`
                    // （双目标只丢一个时断言整条不出现；「跳过该目标保留其余」的
                    // 变异版会让它红）。
                    resolved = false;
                    break;
                };
                let position = *position;
                // 判重键用 `position`，不是「目标身份」`item_id` —— 两者在这条链路上
                // 不是同一个概念（`planner_build` 阶段按 item_id 落目标，这里按会话内
                // 编号判重）。当前等价、可以这么写的**前提**是 position 在会话内唯一
                // （store 的复合主键 + `session_build` 连续生成）。**依赖此前提**：
                // 库若被改出重复 position，这里会把一个不同条目静默吞掉（宁可少列
                // 一个目标，也不要同一行出现两遍 —— 后者会让确认卡片骗用户）。
                if targets.iter().any(|existing| existing.position == position) {
                    continue;
                }
                targets.push(PlannerTarget {
                    item: (*item).clone(),
                    position,
                });
            }
            if !resolved || targets.is_empty() {
                dropped += 1;
                continue;
            }
            // 目标按 position 升序：确认卡片上「第 N、M 条」的列举顺序就是它，
            // 而模型给的顺序是 prompt 序，两套编号混用会指错条目。
            targets.sort_by_key(|target| target.position);
            suggestions.push(PlannerSuggestion {
                // 1-based 连续无空洞：被丢弃的行不占号。
                index: (suggestions.len() + 1) as u64,
                kind: parsed.kind,
                ai_action: parsed.ai_action,
                targets,
                reason: parsed.reason,
            });
        }

        // ⑥ 一行都没活下来、而模型也没明确说「没有建议」→ 这是「模型没按协议输出」，
        // 不是「没有建议」。两种空必须在服务层就分开。
        if suggestions.is_empty() && !plan.explicit_none {
            return Err(PlannerError::SuggestionsUnusable {
                dropped: dropped as usize,
            });
        }

        Ok(PlannerSuggestionSet {
            suggestions,
            dropped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 错误码与重试标志是前端分派文案的依据（照 `session_service.rs` / `agent_service.rs`
    /// 的同名钉子）。两个新码的字符串在 Task 3 才落到命令层，但契约在**这里**就定下：
    /// 谁先写代码谁就得把字符串钉住，免得实现与计划各写一套。
    #[test]
    fn planner_error_codes_and_retryable_flags_are_stable() {
        // 前端按 code 分派引导文案，改动这里等于改 API 契约。
        assert_eq!(PlannerError::SessionGone.code(), "planner_session_missing");
        assert_eq!(
            PlannerError::SuggestionsUnusable { dropped: 3 }.code(),
            "planner_suggestions_unusable"
        );

        // 两个新码各自的 retryable：会话不在了不可重试（重试同一个号还是同一个
        // 结果）；模型没按协议输出可重试（与 ai_invalid_response 同类）。
        assert!(
            !PlannerError::SessionGone.retryable(),
            "会话已经不在了，再点一次不会把它变回来"
        );
        assert!(
            PlannerError::SuggestionsUnusable { dropped: 3 }.retryable(),
            "模型没按协议输出：再试一次可能成功"
        );

        // 透传不重写：会话故障与模型调用失败的 code / retryable 全部原样转出。
        let store = PlannerError::Session(SessionError::StoreUnavailable("boom".into()));
        assert_eq!(store.code(), "ai_session_store_unavailable");
        assert!(!store.retryable());

        let busy = PlannerError::Agent(AgentError::Busy);
        assert_eq!(busy.code(), "ai_busy", "ai_busy 必须逐字透传");
        assert!(busy.retryable(), "等它完成或取消后可以重试");

        let cooldown = PlannerError::Agent(AgentError::Cooldown);
        assert_eq!(cooldown.code(), "ai_cooldown");
        assert!(cooldown.retryable());

        let sensitive = PlannerError::Agent(AgentError::SensitiveContent { position: 2 });
        assert_eq!(sensitive.code(), "ai_sensitive_content");
        assert!(!sensitive.retryable(), "要用户去掉那条内容");

        let not_configured = PlannerError::Agent(AgentError::NotConfigured);
        assert_eq!(not_configured.code(), "ai_not_configured");
        assert!(!not_configured.retryable());
    }
}
