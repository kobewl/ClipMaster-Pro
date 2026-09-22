import { useEffect, useState } from "react";
import type { ClipGroup, PlannerSuggestion } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import {
  describePlannerAction,
  isSupportedPlannerAction,
  plannerActionTitle,
  plannerAiActionLabel,
  plannerTargetNumbers,
} from "@/lib/plannerActions";
import { Icon } from "./Icon";

interface Props {
  open: boolean;
  suggestion: PlannerSuggestion | null;
  /** 界面上的服务名（`getAgentConfig().provider_label`；调用方负责兜底）。 */
  providerLabel: string;
  /** 用户点了确认。`groupId` 只有「加入分组」类才有值，其余三类固定 `null`。 */
  onConfirm: (groupId: string | null) => void;
  onCancel: () => void;
}

/**
 * 副作用确认卡片（Phase 1 第 5 步 · Task 4）：**执行前的最后一道界面**。
 *
 * 为什么是新组件而不是复用 `ConfirmDialog`：后者的语义是「破坏性操作的二次确认」
 * （trash 图标 + danger 按钮 + 「此操作不可撤销」），而白名单四类动作没有一个需要
 * 这个口径 —— 硬套会让「复制」长出一张红色删除卡（关键判断 6）。
 *
 * 四个信息位固定，一格里都不许省（关键判断 6）：动作类型 / 目标条目 / 影响范围 /
 * 建议理由。其中「影响范围」的句子来自 `plannerActions.describePlannerAction`，
 * **本组件一个字的文案都不自己写** —— 逐字措辞见计划 `:346-349`，那是不低报副作用的
 * 唯一定义处。
 *
 * **确认不是安全边界**（关键判断 7）：「用户点过确认」不放行任何本地检查。AI 动作
 * 执行时照旧走 `run_agent_action(_batch)`，敏感内容检测、超长处理、条数上限一个不少。
 * 这张卡片只回答「用户知不知道要发生什么」，不回答「该不该发生」—— 后者由本地门禁
 * 与后端白名单负责。所以本组件**不调任何执行命令**：它只把用户的决定（含所选分组）
 * 交回调用方，执行分派在 AgentFlowDialog。
 */
export function ActionConfirmDialog({
  open,
  suggestion,
  providerLabel,
  onConfirm,
  onCancel,
}: Props) {
  /** `null` = 读取中；`[]` = 读过且确实没有（这两种状态给的界面完全不同）。 */
  const [groups, setGroups] = useState<ClipGroup[] | null>(null);
  const [groupsError, setGroupsError] = useState<string | null>(null);
  const [selectedGroupId, setSelectedGroupId] = useState<string>("");

  const kind =
    suggestion !== null && isSupportedPlannerAction(suggestion.action) ? suggestion.action : null;

  /**
   * 分组列表只在**确实需要**时读一次：`group` 是唯一需要在卡片里选东西的一类，
   * 其余三类与分组无关，不该为一句文案多读一次数据。
   *
   * 依赖用 `open` + 这条建议的**值**（`index` + `action` + 目标 item id）而不是整个
   * 对象：调用方每次渲染都可能给出新的对象身份，按对象依赖会让「读一次」反复触发；
   * 但键里必须带上目标 id —— 否则换了一条目标不同的建议时键不变，上一次选的
   * 分组会留下来，那就成了「把这几条移进了另一个分组」的静默错投。
   */
  const groupFetchKey =
    kind === "group" && suggestion !== null
      ? `${suggestion.index}:${suggestion.action}:${suggestion.targets.map((t) => t.item_id).join(",")}`
      : null;

  useEffect(() => {
    if (!open || groupFetchKey === null) return;
    // 每次打开都从「未选择」开始：卡片是「这一次确认」的界面，不记住上一次的选择。
    setGroups(null);
    setGroupsError(null);
    setSelectedGroupId("");
    let alive = true;
    commands.listGroups().then(
      (next) => {
        if (alive) setGroups(next);
      },
      (err: unknown) => {
        if (!alive) return;
        setGroups([]);
        setGroupsError(isCommandError(err) ? err.message : "读取分组失败，请稍后重试。");
      },
    );
    // 卸载 / 关窗后迟到的响应不落到已经消失的卡片上（照 PreviewDialog 的守卫先例）
    return () => {
      alive = false;
    };
  }, [open, groupFetchKey]);

  // 建议没了或卡片没开就什么都不渲染：调用方不必自己判断，少一处状态分叉。
  // 动作类型不认识时同样不渲染 —— 一张描述不出「会发生什么」的确认卡没有存在意义
  // （Task 5 的可执行判据已经不会给出执行按钮，这里是第二道）。
  if (!open || suggestion === null || kind === null) return null;

  // 收窄后的**本地常量**：`fill` 是个闭包，直接引 prop 的话 TS 不认前面那次
  // null 判断（参数在闭包里被视为可变绑定），类型上会多出一个 `null` 分支。
  const activeSuggestion = suggestion;
  const aiAction = activeSuggestion.ai_action;
  const copy = describePlannerAction(kind);
  const targets = activeSuggestion.targets;
  const selectedGroup = groups?.find((group) => group.id === selectedGroupId) ?? null;

  /** 把模板占位符填成品。只替换模板里真的出现的键，不猜也不补别的。 */
  function fill(template: string): string {
    return template.replace(/\{(\w+)\}/g, (whole, key: string) => {
      switch (key) {
        case "targets":
          return plannerTargetNumbers(targets);
        case "group":
          return selectedGroup?.name ?? whole;
        case "providerLabel":
          return providerLabel;
        case "action":
          // 子动作名（`总结` / `翻译`…）：与标题 `交给 AI · {action}` 用同一个词表
          return plannerAiActionLabel(aiAction);
        default:
          return whole;
      }
    });
  }

  const impactLines = copy.impact
    .split("\n")
    // 未选分组时第 1 句没有主语（组名）—— 不拿「归入「」分组」这样的半句糊弄用户；
    // 后面两句（会移走 / 不删记录）与选不选分组无关，照常显示。
    .filter((line) => !(kind === "group" && selectedGroup === null && line.includes("{group}")))
    .map(fill);

  const alreadyInGroup =
    kind === "group" &&
    selectedGroup !== null &&
    targets.length > 0 &&
    targets.every((target) => target.group_id === selectedGroup.id);

  const canConfirm = kind !== "group" || (selectedGroup !== null && !alreadyInGroup);

  function handleConfirm() {
    if (!canConfirm) return;
    onConfirm(kind === "group" ? selectedGroupId : null);
  }

  return (
    <div
      className="modal-backdrop cm-fade-in"
      role="dialog"
      aria-modal="true"
      aria-labelledby="action-confirm-title"
      onMouseDown={(event) => {
        // 点遮罩 = 放弃这次确认（不执行）。Esc 由 App 统一监听，这里不抢按键。
        if (event.target === event.currentTarget) onCancel();
      }}
    >
      <div className="modal-card action-confirm-card cm-pop-in">
        <header className="modal-header">
          <span className="modal-header__icon">
            <Icon name="checkSquare" />
          </span>
          <div>
            <h2 id="action-confirm-title">{plannerActionTitle(kind, aiAction)}</h2>
            <p>确认后立刻执行这一步</p>
          </div>
          <button type="button" onClick={onCancel} aria-label="关闭" className="icon-button">
            <Icon name="close" />
          </button>
        </header>

        {/* 目标条目：编号用库里的 position、预览用后端下发的预览（与列表行同一规则） */}
        <div className="action-confirm__row">
          <span className="action-confirm__label">目标条目</span>
          <ul className="action-confirm__targets">
            {targets.map((target) => (
              <li key={target.item_id} className="action-confirm__target">
                <span className="action-confirm__index">{target.position}</span>
                <div className="action-confirm__target-text">
                  <p className="action-confirm__preview">{target.preview}</p>
                  <p className="action-confirm__source">{target.source_app ?? "未知来源"}</p>
                </div>
              </li>
            ))}
          </ul>
        </div>

        {/* 影响范围：逐行渲染 plannerActions 的原文（含被动作名吞掉的那部分） */}
        <div className="action-confirm__row">
          <span className="action-confirm__label">影响范围</span>
          <div className="action-confirm__value">
            {impactLines.map((line, index) => (
              // 键用行号：模板是静态的，行号就是它在这一段里的位置。
              // 不用行文本当键 —— 两行万一撞成同一句话，React 会为重复键报警。
              <p key={index} className="action-confirm__line">
                {line}
              </p>
            ))}
            {/* 恢复路径：本期不做撤销，但四类都可逆 —— 这一行要告诉用户怎么退回去 */}
            <p className="action-confirm__undo">{copy.undo}</p>
          </div>
        </div>

        {/* 建议理由：模型给的原文，用户据此判断这一步值不值得做 */}
        <div className="action-confirm__row">
          <span className="action-confirm__label">建议理由</span>
          <p className="action-confirm__value">{activeSuggestion.reason}</p>
        </div>

        {/* 卡片里唯一的输入：目标分组由**用户**决定，模型不猜（关键判断 4） */}
        {kind === "group" && (
          <div className="action-confirm__row">
            <label className="action-confirm__label" htmlFor="action-confirm-group">
              目标分组
            </label>
            <select
              id="action-confirm-group"
              className="action-confirm__select"
              value={selectedGroupId}
              disabled={groups === null || groups.length === 0}
              onChange={(event) => setSelectedGroupId(event.target.value)}
            >
              <option value="">选择一个分组…</option>
              {(groups ?? []).map((group) => (
                <option key={group.id} value={group.id}>
                  {group.name}
                </option>
              ))}
            </select>

            {groups !== null && groups.length === 0 && groupsError === null && (
              <p className="action-confirm__hint">
                还没有分组 —— 先到工具栏的「管理分组」里建一个，再回来执行这条建议。
              </p>
            )}
            {alreadyInGroup && (
              <p className="action-confirm__hint">这些记录已经在这个分组里了。</p>
            )}
            {groupsError !== null && (
              <p role="alert" className="action-confirm__error">
                ⚠ {groupsError}
              </p>
            )}
          </div>
        )}

        <div className="modal-footer">
          <button type="button" onClick={onCancel} className="button button--secondary">
            取消
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={!canConfirm}
            className="button button--primary"
          >
            确认执行
          </button>
        </div>
      </div>
    </div>
  );
}
