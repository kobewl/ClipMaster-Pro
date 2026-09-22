import type { PlannerSuggestionKind, PlannerTarget } from "@/types/clipboard";
import { AGENT_ACTION_LABELS, isAgentAction } from "@/types/clipboard";

/**
 * Planner 四类动作的**展示表**：动作标题、影响范围、恢复路径。
 *
 * 为什么单独成文件：这四类动作的文案是本阶段「**不低报副作用**」这条规则的唯一
 * 落地处（关键设计判断 6）—— 复制隐含 `insert_or_touch`（复制时间会变）、粘贴隐含
 * 隐藏窗口与投 ⌘V、AI 动作隐含外发与计费。这些平常会被动作名吞掉的部分必须在确认
 * 卡片上逐字写出来，所以句子只能有一个来源：改这里就是改所有地方的显示。
 *
 * 另一道防线在 `isSupportedPlannerAction`：**不认识的类型一律不可执行**（前端第二道，
 * 后端 `planner_build` 才是权威闸门）。删除 / 清空 / 改设置类动作永不入列 —— 它们
 * 没有恢复路径，一旦要进白名单，撤销机制就是前置条件（关键判断 8）。
 */

/**
 * 白名单四类，**镜像后端** `planner_build::PLANNER_ACTION_KEYS`（那边的唯一入口）。
 *
 * 同源风险：改一边必须改另一边。后果不对称 —— 后端多一类而这里少一类，那一类建议
 * 只会显示成「当前版本不支持这个动作」（不可执行，安全）；反过来这里多一类而后端
 * 不认，界面会给出一个点了必被后端丢弃的按钮。
 */
export const PLANNER_ACTION_KINDS: readonly PlannerSuggestionKind[] = [
  "copy",
  "paste",
  "group",
  "ai",
];

/**
 * 这个动作类型能不能执行。
 *
 * 未知类型返回 `false`（→ 不可执行、不渲染执行按钮），**不猜、不兜底成别的动作**：
 * 把 `delete` 兜底成 `copy` 这类「贴心」处理，是模型（或注入）能拿到执行权的最短
 * 路径。参数收 `string`（DTO 下发的就是字符串），在调用处完成窄化。
 */
export function isSupportedPlannerAction(value: string): value is PlannerSuggestionKind {
  return PLANNER_ACTION_KINDS.some((kind) => kind === value);
}

/**
 * 确认卡片「影响范围」与「恢复路径」的文案模板。
 *
 * `impact` 里的 `\n` 是**行分隔**：计划 `:346-349` 用 `+` 串起来的每一段各占一行，
 * 卡片逐行渲染（一段里可能不止一个句号，所以不能按句号切）。`{targets}` /
 * `{group}` / `{providerLabel}` / `{action}` 由确认卡片填成品 —— 模板里留占位符而不是
 * 在这里拼字符串，是为了让「句子」与「填进去的数据」分开：句子只有一个来源。
 */
export interface PlannerActionCopy {
  /** 动作类型这一格。`ai` 类的 `{action}` 由 `plannerActionTitle` 填。 */
  title: string;
  /** 影响范围：如实写全套副作用，含被动作名吞掉的那部分（逐行）。 */
  impact: string;
  /** 怎么退回去。四类都可逆 / 零损害，所以**四句都不含「不可撤销」**。 */
  undo: string;
}

/** 四类动作的文案。`Record<PlannerSuggestionKind, …>` 让加第 5 类时 TS 直接报错。 */
const PLANNER_ACTION_COPY: Record<PlannerSuggestionKind, PlannerActionCopy> = {
  copy: {
    title: "复制到剪贴板",
    impact:
      "会把第 {targets} 条内容写入系统剪贴板。同时会更新它在历史中的复制时间 —— 这条记录的「最近复制」时刻变成现在，列表按最新在前，它可能因此上移。\n" +
      "不改动内容本身，也不删除任何记录。",
    undo: "没有要退回的东西：内容没改、记录没删，只是它自己的「最近复制」时刻变成了现在。",
  },
  paste: {
    title: "粘贴到当前应用",
    impact:
      "会先把第 {targets} 条内容写入系统剪贴板，再隐藏本窗口，并向当前应用投一次 Command+V（等同你手动按一下粘贴）。\n" +
      "没有授予「辅助功能」权限时 Command+V 可能失败 —— 那时内容已经在剪贴板里（与列表里的「粘贴」按钮行为完全一致）。",
    undo: "只影响当前应用里那一次输入 —— 按 Command+Z 可以撤回；剪贴板里的内容会在复制别的记录时自然被替换。",
  },
  group: {
    title: "加入分组",
    impact:
      "会把第 {targets} 条记录归入「{group}」分组。\n" +
      "原来属于别的分组的记录会一并移过去（一条记录只能在一个分组里）。\n" +
      "不删除任何记录，随时可以在分组菜单里改回来。",
    undo: "在分组菜单里把它们移走（或移到别的分组）就退回去了，一条记录都不会少。",
  },
  ai: {
    title: "交给 AI · {action}",
    impact:
      "会把第 {targets} 条内容发送给 {providerLabel} 做一次「{action}」（一次模型调用，可能产生费用）。\n" +
      "结果只作为草稿显示在会话页里，不会改写原始记录。\n" +
      "若其中含敏感内容，本地会直接拦下，不发送。",
    undo: "不满意就把那张草稿丢掉；原始记录、分组与剪贴板都保持原样。",
  },
};

/** 取一类动作的文案模板。未知类型在调用前就该被 `isSupportedPlannerAction` 挡掉。 */
export function describePlannerAction(kind: PlannerSuggestionKind): PlannerActionCopy {
  return PLANNER_ACTION_COPY[kind];
}

/**
 * `ai` 类建议的子动作名（`总结` / `翻译` …）。
 *
 * 认不出来的 key **原样显示**（照 `AgentRunDto.action_label` 的先例：宁可露出机器名，
 * 也不假装它是另一个动作）；`null` 给一个破折号占位 —— 后端对 `ai` 类必带合法 key，
 * 这条路径只在协议被绕过时才会走到。
 */
export function plannerAiActionLabel(aiAction: string | null): string {
  if (isAgentAction(aiAction)) return AGENT_ACTION_LABELS[aiAction];
  return aiAction ?? "—";
}

/** 动作标题的成品（`ai` 类带上子动作名，词表与预览窗口的按钮文案同词）。 */
export function plannerActionTitle(
  kind: PlannerSuggestionKind,
  aiAction: string | null,
): string {
  return describePlannerAction(kind).title.replace("{action}", plannerAiActionLabel(aiAction));
}

/**
 * 目标编号串：**按 `position` 升序**、顿号连接（`4` / `3、7`）。
 *
 * 「第 N、M 条」里的 N、M 就是它 —— 用 `position`（会话成员在库里的号）而不是数组
 * 下标：界面上的号就是库里的号，指错条目是这张卡片最严重的失效方式（关键判断 6）。
 * 升序是刻意的：DTO 的目标顺序照服务层原样透传，而「第 3、7 条」这种列举读起来必须
 * 从小到大。
 */
export function plannerTargetNumbers(targets: PlannerTarget[]): string {
  return targets
    .map((target) => target.position)
    .sort((left, right) => left - right)
    .join("、");
}

/**
 * 把后端 `code` + `message` 变成 Planner 面板上要显示的一句话。
 *
 * 只有两条码需要语境化，且**只加一句语境、不改后端原文**（关键判断 11）：单飞闸门
 * 是跨路径全局的，预览窗口正在跑总结时 Planner 必然撞 `ai_busy` —— 但「已有一次
 * AI 请求正在进行」在这个语境里指代不清（用户可能刚点完另一个窗口的按钮）。所以在
 * 前面补一句说清「是谁在跑」，**原文作为第二行保留**（原文里的「等它完成或取消」
 * 正是下一步动作）。
 *
 * 其余码**原样返回**：后端 message 已经是给用户看的话，这里再改写一遍就会出现
 * 两份口径（本阶段的约束是 code 与 message 一个字不改）。
 */
export function plannerRunErrorMessage(code: string, message: string): string {
  if (code === "ai_busy") {
    return (
      "此刻有另一次 AI 请求在跑（比如预览窗口里的总结或翻译）；等它完成或取消后再点「下一步建议」。\n" +
      message
    );
  }
  if (code === "ai_cooldown") {
    return `上一次请求刚发出，间隔一小会儿再试。\n${message}`;
  }
  return message;
}
