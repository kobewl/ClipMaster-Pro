import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type {
  AgentAction,
  AgentResult,
  AgentSessionDetail,
  AgentSessionSummary,
  PlannerSuggestion,
  PlannerSuggestionKind,
  PlannerSuggestionSet,
  PlannerTarget,
} from "@/types/clipboard";
import { isAgentAction, isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import { createRequestId } from "@/lib/requestId";
import {
  isSupportedPlannerAction,
  plannerActionTitle,
  plannerAiActionLabel,
  plannerRunErrorMessage,
  plannerTargetNumbers,
} from "@/lib/plannerActions";
import { ActionConfirmDialog } from "./ActionConfirmDialog";
import { AgentResultCard } from "./AgentResultCard";
import { ConfirmDialog } from "./ConfirmDialog";
import { Icon } from "./Icon";

interface Props {
  open: boolean;
  onClose: () => void;
  /**
   * AI 未配置 / 地址不对时，从建议面板直接跳到设置面板里的 AI 助手。
   *
   * 分寸（关键判断 12）：列表与详情本身**不依赖 API Key**（会话纯本地算出），
   * 所以这条路径只在「下一步建议」真的撞上 `ai_not_configured` /
   * `ai_invalid_base_url` 时才出现，不是一进门就摆一个「去设置」。
   * 照 `App.tsx` 对批量入口的写法：由 App 的回调先关掉工作台再打开设置。
   */
  onOpenSettings: () => void;
}

/**
 * 空状态文案：解释**分组规则**，不是一句「暂无数据」。
 *
 * 用户看到空列表时最需要知道的是「怎么才有会话」，所以把三条硬条件里他能直接
 * 影响的两条（至少 2 条、时间相邻 ≤30 分钟）写出来。
 * 空列表刻意不给「去设置」的路径：会话由本地字段算出、不调模型，空列表与
 * API Key 无关 —— 需要服务配置的只有「下一步建议」，那条路径在建议面板里。
 */
const EMPTY_HINT =
  "最近 3 天里没有可以归组的内容 —— 会话至少需要 2 条时间相邻（≤30 分钟）且同源或共享关键词的记录。";

/** 执行成功后给的那一句（四类各一句：说的是刚发生的事，不是动作名）。 */
const EXECUTION_NOTICE: Record<PlannerSuggestionKind, string> = {
  copy: "已复制到剪贴板 ✓",
  paste: "已粘贴到当前应用 ✓",
  group: "已加入分组 ✓",
  ai: "已生成草稿 ✓",
};

/**
 * 可执行建议的**收窄类型**：`ai` 类的 `ai_action` 在这里才被当成 `AgentAction`。
 *
 * 分成两支而不是一刀切成 `ai_action: AgentAction`：非 AI 类的 `ai_action` 是显式
 * `null`（后端 DTO 如此），把它说成 `AgentAction` 是假话 —— 类型只在它真的成立的
 * 地方写明，`execute` 里才能既不用 `as` 也拿得到合法的联合类型。
 */
type ExecutablePlannerSuggestion = PlannerSuggestion &
  (
    | { action: Exclude<PlannerSuggestionKind, "ai"> }
    | { action: "ai"; ai_action: AgentAction }
  );

/**
 * 一条建议是否**可执行** —— 全前端唯一的执行判据（白名单的第二道，后端
 * `planner_build` 才是权威闸门）。
 *
 * 两个条件都要过：动作类型在白名单四类里，且 `ai` 类的 `ai_action` 是已知动作 key
 * （`isAgentAction` 走 `Object.hasOwn`，原型链上的 key 不算）。
 *
 * 为什么写成**类型谓词**而不是「返回 boolean 再由调用方 `as` 硬转」：`runAgentAction`
 * 的签名要的是 `AgentAction` 联合类型，而 DTO 下发的是 `string`，不窄化就传不进去。
 * `as` 能把编译糊过去，代价是把「模型给的任意字符串」直接当成合法动作送进执行命令。
 * 窄化失败的正确答案是这一行**显示成不支持**（不渲染执行按钮），不是硬转后照发。
 */
function isExecutablePlannerSuggestion(
  suggestion: PlannerSuggestion,
): suggestion is ExecutablePlannerSuggestion {
  return (
    isSupportedPlannerAction(suggestion.action) &&
    (suggestion.action !== "ai" || isAgentAction(suggestion.ai_action))
  );
}

/**
 * 建议行的标题：四类用 `plannerActions` 的文案（`ai` 类带子动作名）；认不出来的
 * 动作类型**原样露出 key**（照 `AgentRunDto.action_label` 的先例：宁可显示机器名，
 * 也不假装它是另一个动作 —— 兜底成别的动作正是模型拿到执行权的最短路径）。
 */
function suggestionTitle(suggestion: PlannerSuggestion): string {
  if (isSupportedPlannerAction(suggestion.action)) {
    return plannerActionTitle(suggestion.action, suggestion.ai_action);
  }
  return suggestion.ai_action === null
    ? suggestion.action
    : `${suggestion.action} · ${plannerAiActionLabel(suggestion.ai_action)}`;
}

/**
 * 目标一览：形如 `第 3、7 条 · 预览一；预览二`。
 *
 * 编号直接复用 `plannerTargetNumbers`（与确认卡片同一份「第 N、M 条」的来源），
 * 预览**按同一顺序**排 —— 编号升序而预览照数组原序的话，用户会把预览配错号，
 * 而「指错条目」是这一行最严重的失效方式（关键判断 6）。
 */
function targetSummary(targets: PlannerTarget[]): string {
  if (targets.length === 0) return "";
  const previews = [...targets]
    .sort((left, right) => left.position - right.position)
    .map((target) => target.preview || "（图片）")
    .join("；");
  return `第 ${plannerTargetNumbers(targets)} 条 · ${previews}`;
}

/**
 * 绝对时间 `MM-DD HH:MM`（本地时区）。
 *
 * 会话是「一段时段」，绝对时间的信息量比「N 分钟前」大；也因此不必再开一个
 * 每分钟刷新的时钟（相对时间才需要，见 Global Constraints 的定时器约束）。
 * 不用 `toLocaleString` 拼：那会带出各区域不同的分隔符，这里要的是固定格式。
 */
function formatSessionTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/**
 * 穷尽性检查：白名单加了第五类动作、而 `execute` 忘了给它分派时，**编译期**在这里
 * 报错（`default` 分支里的值类型是 `never`，任何别的类型都赋不进去）。
 *
 * 写在模块层而不是内联 `const unhandled: never = suggestion.action`：后者在
 * 判别联合被穷尽后整个对象已经被收窄成 `never`，再取 `.action` 属性 TS 会直接报
 * 「属性不存在」—— 检查本身没写成，先炸在语法上。
 */
function unhandledPlannerAction(value: never): never {
  throw new Error(`未分派的建议动作：${String(value)}`);
}

/**
 * 一个在途请求：号 + **它是在哪个会话作用域里发出的**。
 *
 * 作用域不是装饰。取消挂在「离开这个会话 / 关掉工作台」那一轮的清理函数上，而
 * React 也会为**进入**会话那一轮跑一次清理（依赖从 `undefined` 变成会话 id），
 * 且那次清理可能跑在用户点击**之后**。没有作用域标记的话，用户刚点出去的那个
 * 请求会被这一次「进入」的清理顺手取消掉 —— 界面表现是「点了没反应」，审计里
 * 还留下一条假的 `ai_cancelled`。带上作用域之后，清理函数只取消「属于正被离开
 * 的那一轮」的请求，进入那一轮不会误伤。
 */
interface InFlightRun {
  requestId: string;
  scope: string;
}

/** 列表也是一个作用域（详情还没打开）；工作台关掉时清理函数照常跑到。 */
const LIST_SCOPE = "list";

/** 当前会话作用域：有详情看详情，否则就是列表。 */
function sessionScope(detail: AgentSessionDetail | null): string {
  return detail?.id ?? LIST_SCOPE;
}

/**
 * 建议面板的状态。`sessionId` 是**产它的那个会话**，不是装饰：
 *
 * 只有属于当前会话的那一份才渲染（见组件里 `suggestions` 的推导）。为什么不靠
 * 「换会话时清一次状态」的 effect 来保证同一件事：那样就把正确性押在 effect 的
 * **执行时机**上 —— 而进入一个会话那一轮的被动 effect 可能跑在用户点击之后
 * （React 的调度器决定何时刷 effect，不在点击那一帧同步跑完），它会把用户刚点
 * 出去的那次请求的状态连同结果一起清掉，界面表现就是「点了没反应」，还可能把
 * 在途请求误取消、在审计里留下假的 `ai_cancelled`。带上会话标记之后，「属于别的
 * 会话」在渲染那一刻就被挡掉，与 effect 何时跑无关。
 */
interface SuggestionPanelState {
  sessionId: string;
  /** `null` = 这次点出去还没回来；`{ suggestions: [] }` = 读过且模型明确说没有。 */
  set: PlannerSuggestionSet | null;
  running: boolean;
  /** 已经语境化过的文案（`ai_busy` 的第一行是前端补的，原文作为第二行保留）。 */
  error: string | null;
  /** 错误码单独记着：`ai_not_configured` 要在旁边给一个「去设置」的入口。 */
  errorCode: string | null;
}

/** 执行侧的状态（草稿 / 错误 / 确认卡片里的那一条），同样带会话标记，理由同上。 */
interface ExecutionPanelState {
  sessionId: string;
  result: AgentResult | null;
  running: boolean;
  error: string | null;
  errorCode: string | null;
  copied: boolean;
  /** 正在确认的那一条（确认卡片开着时非 null）。 */
  pending: PlannerSuggestion | null;
}

/**
 * Flow 会话工作台：把时间相邻、说得清关系的片段看成一「段」。
 *
 * 单栏两级（列表 → 详情），详情里逐条给出**理由** —— 这是本功能与「聪明的自动
 * 关联」的分界线：凡是自动成组，每一条都必须能说出为什么（`reason`），说不清
 * 就不该成组。用户手动存的会话例外：他为什么放在一起不需要机器解释，那些成员
 * 显示「手动保存」。
 *
 * 会话本身仍然**不调模型**（纯本地派生）；详情里多出来的「下一步建议」是用户
 * **显式点**的那个按钮才产生一次模型调用（拍板 1/3：打开工作台不等于授权花钱）。
 * 模型给回来的只是**建议**：执行与否由用户在确认卡片上点下确认决定 —— 这个组件
 * 不含任何「模型说执行就执行」的路径，执行分派只有一个入口
 * （`handleConfirmExecution` → `execute`）。
 */
export function AgentFlowDialog({ open, onClose, onOpenSettings }: Props) {
  /** null = 还在读；[] = 读过且确实没有。这两种状态给的界面完全不同。 */
  const [sessions, setSessions] = useState<AgentSessionSummary[] | null>(null);
  const [detail, setDetail] = useState<AgentSessionDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // ---- 下一步建议（Task 5）----
  const [suggestionPanel, setSuggestionPanel] = useState<SuggestionPanelState | null>(null);
  const [executionPanel, setExecutionPanel] = useState<ExecutionPanelState | null>(null);
  /**
   * 确认卡片上的服务名。**按需读**：只有 `ai` 类会把它写进影响范围，其余三类与
   * 「下一步建议」本身都跟服务名无关 —— 不该为一句文案在打开工作台时多读一次配置。
   * 读不到就保持这个默认文案（照 PreviewDialog 的兜底）。
   */
  const [providerLabel, setProviderLabel] = useState("DeepSeek");

  /**
   * 两个在途请求，**各自独立、互不覆盖**：建议与执行是两次彼此独立的请求，
   * 各按各的号取消、各按各的号守卫（照 PreviewDialog 的单号模式各来一份）。
   * 合成一个号会造出「取消了建议把执行的守卫也一起作废」这类错配：
   * 建议的号一变，执行那条迟到结果就会被误判成过期。
   */
  const plannerRunRef = useRef<InFlightRun | null>(null);
  const actionRunRef = useRef<InFlightRun | null>(null);
  // 异步执行返回时读取最新会话，避免把旧会话的成功提示挂到用户刚切换过去的详情上。
  const currentSessionIdRef = useRef<string | null>(null);
  useLayoutEffect(() => {
    currentSessionIdRef.current = detail?.id ?? null;
  }, [detail?.id]);

  const loadSessions = useCallback(async () => {
    setError(null);
    try {
      // 这一次调用同时触发服务端重算（关键判断 10）：会话只在用户打开工作台或
      // 显式保存时计算，不做后台轮询，也没有单独的 rebuild 命令可调。
      const next = await commands.listAgentSessions();
      setSessions(next);
    } catch (err: unknown) {
      setSessions([]);
      setError(isCommandError(err) ? err.message : "读取会话失败，请稍后重试。");
    }
  }, []);

  // 每次打开都回到列表并重读一遍：关掉再打开不该看到上一次的详情或提示。
  // 建议与执行的两份界面态一并清掉 —— 建议不落库（关键判断 5），关掉即弃，
  // 重开时看到的必须是一片空白，而不是上次那一份过期快照。
  useEffect(() => {
    if (!open) return;
    setDetail(null);
    setConfirmDeleteOpen(false);
    setError(null);
    setNotice(null);
    setSessions(null);
    setSuggestionPanel(null);
    setExecutionPanel(null);
    void loadSessions();
  }, [open, loadSessions]);

  /**
   * 离开当前会话（回列表 / 换会话 / 关掉工作台 / 卸载）时，把还在飞的两个请求
   * **都**按号取消。两条理由：用户已经不打算看这次结果了 —— 让它在后台跑完等于
   * 白计费；而结果真回来也没地方显示，只会变成串台隐患。
   *
   * 依赖 `open` 与 `detail?.id`：执行成功后重读详情（**同一个 id**）不是「离开」，
   * 绝不能因此取消 —— 那会在审计里留下假的 ai_cancelled。
   *
   * 只取消**属于正被离开的那一轮**的请求（`scope !== leaving`）：依赖变化时 React
   * 会为新旧两轮各跑一次，进入会话那一轮的清理跑在用户点击之后时，不带作用域判断
   * 就会把刚点出去的请求顺手取消掉（界面表现是「点了没反应」）。
   * 先清 ref 再发取消：清理可能因依赖变化再进一次，ref 空了就不会重复发一轮。
   */
  useEffect(() => {
    if (!open) return;
    const leaving = sessionScope(detail);
    return () => {
      for (const ref of [plannerRunRef, actionRunRef]) {
        const run = ref.current;
        if (!run || run.scope !== leaving) continue;
        ref.current = null;
        // 取消失败不致命：请求会自然结束，过期响应守卫会挡住它的结果。
        void commands.cancelAgentAction(run.requestId).catch(() => {});
      }
      // 属于这个会话的界面态也一起收掉：请求已经被取消，不会再有结果回来，
      // 留着只会让用户回到这个会话时看到一个永远转不完的「取消」按钮。
      setSuggestionPanel((current) => (current?.sessionId === leaving ? null : current));
      setExecutionPanel((current) => (current?.sessionId === leaving ? null : current));
    };
    // 依赖里用 `detail?.id` 而不是 `detail` 对象：详情每次刷新都是新对象，
    // 按对象依赖会让用户在系统里复制任何东西都触发一轮「离开」。
  }, [open, detail?.id]);

  // 只有属于**当前**会话的那一份建议 / 执行状态可以渲染。
  //
  // 这里是「换会话就看不到上一个会话的建议」这条规则的落地处，也是重算规则的
  // 落地处：建议是点按钮那一刻的快照，回来时不会自动重算 —— 想刷新就再点一次
  // （拍板 1：重算只发生在用户点按钮那一下）。
  const suggestions =
    suggestionPanel !== null && detail !== null && suggestionPanel.sessionId === detail.id
      ? suggestionPanel
      : null;
  const execution =
    executionPanel !== null && detail !== null && executionPanel.sessionId === detail.id
      ? executionPanel
      : null;

  if (!open) return null;

  async function openSession(id: string) {
    setError(null);
    setNotice(null);
    setDetailLoading(true);
    try {
      const next = await commands.getAgentSession(id);
      if (next === null) {
        // 会话可能已经随它最后一条成员一起被删掉（触发器带走）—— 这是设计的正常
        // 结果而不是错误：回列表、把话说明白、顺手重读一次。
        setNotice("这个会话已经不在了（它的记录可能已被删除）。");
        await loadSessions();
        return;
      }
      setDetail(next);
    } catch (err: unknown) {
      setError(isCommandError(err) ? err.message : "读取会话详情失败，请稍后重试。");
    } finally {
      setDetailLoading(false);
    }
  }

  /**
   * 删除当前会话。只删派生数据：原始剪贴板记录一条不动（后端语义）。
   *
   * 删完回列表并重读 —— 列表里还挂着一条已经不存在的会话，比任何提示都更容易误导。
   */
  async function handleDelete() {
    if (!detail || deleting) return;
    setConfirmDeleteOpen(false);
    setDeleting(true);
    setError(null);
    try {
      await commands.deleteAgentSession(detail.id);
      setDetail(null);
      setNotice("已删除这个会话 ✓");
      await loadSessions();
    } catch (err: unknown) {
      setError(isCommandError(err) ? err.message : "删除会话失败，请稍后重试。");
    } finally {
      setDeleting(false);
    }
  }

  /**
   * 求一次「下一步建议」。**只在用户点按钮时发生**（拍板 1/3）：打开工作台、
   * 进详情、切会话都不触发，所以这里的调用要等按钮那一下。
   *
   * 建议不落库（关键判断 5）：返回值就是全部，面板上的列表是打开时的快照，
   * 关掉工作台即弃，也没有任何后台刷新会覆盖它。
   */
  async function handleSuggest() {
    if (detail === null || suggestions?.running === true) return;
    const sessionId = detail.id;
    const requestId = createRequestId();
    plannerRunRef.current = { requestId, scope: sessionId };
    setSuggestionPanel({ sessionId, set: null, running: true, error: null, errorCode: null });
    // 上一次的草稿跟着一起下：换了建议集还挂着旧草稿，就成了两组结果的拼接。
    updateExecution(sessionId, {
      result: null,
      error: null,
      errorCode: null,
      copied: false,
    });
    try {
      const next = await commands.suggestSessionActions(sessionId, requestId);
      // 过期响应守卫：换了会话（或已经取消）之后回来的建议不许渲染。
      if (plannerRunRef.current?.requestId !== requestId) return;
      setSuggestionPanel((current) =>
        current !== null && current.sessionId === sessionId
          ? { ...current, set: next, running: false }
          : current,
      );
    } catch (err: unknown) {
      // 失败路径同样要守卫：否则上一个会话的报错会挂在新会话上。
      if (plannerRunRef.current?.requestId !== requestId) return;
      const code = isCommandError(err) ? err.code : null;
      const message = isCommandError(err)
        ? // 语境化只在显示层：`ai_busy` 说清「是谁在跑」，后端原文作为第二行保留
          // （关键判断 11，码与文案一个字不改）。
          plannerRunErrorMessage(err.code, err.message)
        : "获取建议失败，请稍后重试。";
      setSuggestionPanel((current) =>
        current !== null && current.sessionId === sessionId
          ? { ...current, running: false, error: message, errorCode: code }
          : current,
      );
    } finally {
      // 只有「还是这次请求」才收尾；过期的由「离开那个会话」那一轮负责清状态。
      if (plannerRunRef.current?.requestId === requestId) plannerRunRef.current = null;
    }
  }

  /**
   * 取消在途的建议请求（请求期间按钮变「取消」）。带号取消：后端只取消号匹配的
   * 那一次。取消后后端让那次请求以 `ai_cancelled` 结束，正常错误路径会把它显示
   * 出来（ref 里仍是这个号，守卫不会挡掉）。
   */
  async function handleCancelSuggest() {
    const run = plannerRunRef.current;
    if (run === null) return;
    try {
      await commands.cancelAgentAction(run.requestId);
    } catch {
      // 取消失败不致命：请求会自然结束，过期响应守卫会挡住它的结果
    }
  }

  /**
   * 点某条建议的「执行」：**只把确认卡片叫出来，这一步不执行任何东西**。
   *
   * 唯一的例外是服务名：只有 `ai` 类会把它写进卡片的影响范围，所以只在这一刻按需
   * 读一次配置，读完即用（读不到保持默认文案，不影响执行本身 —— 真正的错误会在
   * 执行时报出）。
   */
  function handleRequestExecute(suggestion: PlannerSuggestion) {
    if (detail === null) return;
    if (suggestion.action === "ai") {
      commands
        .getAgentConfig()
        .then((info) => setProviderLabel(info.provider_label))
        .catch(() => {
          /* 读不到就保持默认文案 */
        });
    }
    updateExecution(detail.id, { pending: suggestion, error: null, errorCode: null });
  }

  /**
   * 执行成功后的收尾：**重读当前会话详情** + 给一句提示。
   *
   * 重读用函数式更新而不是直接 `setDetail(next)`：await 之后用户可能已经切到了
   * 别的会话，而闭包里的 `detail` 是发起那一刻的旧值 —— 直接赋值会把界面从他
   * 正看的会话拽回旧的那个。函数式更新读到的是当下状态，界面仍停在这个会话上才落笔。
   *
   * 顺序不能反：提示放在重读之后，否则会被重读里那套「先清提示」的逻辑顺手清掉。
   * 重读失败不覆盖执行结果 —— 那一笔副作用真的发生了，提示必须留着。
   */
  async function refreshAfterExecution(sessionId: string, message: string) {
    try {
      const next = await commands.getAgentSession(sessionId);
      setDetail((current) => (current?.id === sessionId && next !== null ? next : current));
    } catch {
      /* 重读失败不致命 */
    }
    if (currentSessionIdRef.current === sessionId) setNotice(message);
  }

  /**
   * 修订某个会话的执行侧状态（不传的字段保持原值）。建议侧与执行侧各一份状态，
   * 各自带会话标记，写法也各来一份小工具 —— 比在十处调用点各写一遍展开运算清楚。
   *
   * **只在同步路径上用这个**（点击处理函数里，`sessionId` 一定就是当前会话）：
   * 它会在会话对不上时**新建**一份，因此不能用于 await 之后的写入。
   */
  function updateExecution(
    sessionId: string,
    patch: Partial<Omit<ExecutionPanelState, "sessionId">>,
  ) {
    setExecutionPanel((current) => {
      const base: ExecutionPanelState =
        current !== null && current.sessionId === sessionId
          ? current
          : {
              sessionId,
              result: null,
              running: false,
              error: null,
              errorCode: null,
              copied: false,
              pending: null,
            };
      return { ...base, ...patch };
    });
  }

  /**
   * await 之后写执行侧状态：**会话对不上就什么都不做**。
   *
   * 两个理由，缺一不可：
   * 1. 用户可能已经切到别的会话 —— 这一笔的草稿 / 报错不该落到他的新界面上；
   * 2. 更不能把新会话那一份执行状态**顶掉**（`updateExecution` 在会话对不上时
   *    会新建，正好是这里要避免的：新会话自己的草稿与报错会被无声抹掉）。
   */
  function patchExecution(
    sessionId: string,
    patch: Partial<Omit<ExecutionPanelState, "sessionId">>,
  ) {
    setExecutionPanel((current) =>
      current !== null && current.sessionId === sessionId ? { ...current, ...patch } : current,
    );
  }

  /**
   * 执行一条建议 —— **全组件唯一的执行分派点**，四类各自复用既有命令
   * （关键判断 13：不为 Planner 新开「执行命令」，同一件事只该有一份实现）。
   *
   * 两个不变量写在这里：
   * 1. 进得来的一定是可执行的建议（`isExecutablePlannerSuggestion` 已窄化，不用 `as`）；
   * 2. 调用它的只有「确认卡片上点下确认」那一下 —— 没有别的入口，所以
   *    「未确认前零执行调用」是结构性的，不靠调用方自觉。
   *
   * 「用户点过确认」**不放行**任何本地门禁（关键判断 7）：`ai` 类走既有的
   * `run_agent_action(_batch)`，敏感内容检测、超长处理、条数上限一个不少。
   */
  async function execute(suggestion: PlannerSuggestion, groupId: string | null) {
    if (!isExecutablePlannerSuggestion(suggestion)) return;
    const targets = suggestion.targets;
    const sessionId = detail?.id ?? null;
    // 执行分派发生在哪个会话里，收尾就只认哪个会话 —— 中途换了会话的话，
    // 这一笔的草稿与提示都不该落到新会话的界面上。
    const runSessionId = sessionId ?? LIST_SCOPE;
    updateExecution(runSessionId, {
      running: true,
      error: null,
      errorCode: null,
      result: null,
      copied: false,
      pending: null,
    });
    // 这一笔 AI 执行自己的请求号：只有它等于 ref 里那个号时才收尾 / 落笔，
    // 别的动作（copy / paste / group）不碰这个 ref —— 否则它们的收尾会把一个
    // 仍在飞的 AI 请求的守卫号顺手清掉，那次结果就会被静默丢弃。
    let aiRequestId: string | null = null;
    try {
      switch (suggestion.action) {
        case "copy":
          // 单目标（后端白名单保证恰好 1 条：剪贴板一次只有一份内容）。
          await commands.copyClipboardItem(targets[0].item_id);
          break;
        case "paste":
          await commands.pasteClipboardItem(targets[0].item_id);
          break;
        case "group":
          // 分组由**用户**在确认卡片里选（模型不指定分组，关键判断 4）；
          // 逐目标调用就是「一条记录只能在一个分组里」这条约束的落地点。
          if (groupId === null) return;
          for (const target of targets) {
            await commands.setItemGroup(target.item_id, groupId);
          }
          break;
        case "ai": {
          const requestId = createRequestId();
          aiRequestId = requestId;
          actionRunRef.current = { requestId, scope: runSessionId };
          const itemIds = targets.map((target) => target.item_id);
          // 1 条走单条语义（超长报错而不是静默截断），多条走批量（与批量入口同一约束）。
          const result =
            itemIds.length === 1
              ? await commands.runAgentAction(itemIds[0], suggestion.ai_action, requestId)
              : await commands.runAgentActionBatch(itemIds, suggestion.ai_action, requestId);
          // 过期响应守卫：离开了这个会话（或已经取消）之后回来的结果不许渲染。
          if (actionRunRef.current?.requestId !== requestId) return;
          patchExecution(runSessionId, { result });
          break;
        }
        default: {
          // 第五类动作加进白名单却忘了在这里分派：编译期就挡住
          unhandledPlannerAction(suggestion);
        }
      }
    } catch (err: unknown) {
      patchExecution(runSessionId, {
        error: isCommandError(err) ? err.message : "执行失败，请稍后重试。",
        errorCode: isCommandError(err) ? err.code : null,
      });
      return;
    } finally {
      // 只有「还是这次 AI 执行」才收尾；过期的由「离开那个会话」那一轮负责清状态。
      if (aiRequestId !== null && actionRunRef.current?.requestId === aiRequestId) {
        actionRunRef.current = null;
      }
      patchExecution(runSessionId, { running: false });
    }
    // 只有走到这里才是执行成功（失败在上面 return 了）。
    if (sessionId !== null) {
      await refreshAfterExecution(sessionId, EXECUTION_NOTICE[suggestion.action]);
    }
  }

  /**
   * 确认卡片上点下确认 —— **唯一**会走到执行分派的入口。
   * 卡片同时收起来：确认过的那一步不该因为卡片还开着被点第二次。
   */
  function handleConfirmExecution(groupId: string | null) {
    const suggestion = execution?.pending ?? null;
    if (detail === null) return;
    updateExecution(detail.id, { pending: null });
    if (suggestion === null) return;
    void execute(suggestion, groupId);
  }

  /** 复制 AI 草稿（走既有命令，不写入历史）。 */
  async function handleCopyActionResult() {
    if (detail === null || execution?.result == null) return;
    try {
      await commands.copyTextToClipboard(execution.result.content);
      updateExecution(detail.id, { copied: true });
    } catch {
      updateExecution(detail.id, { error: "复制 AI 结果失败。" });
    }
  }

  return (
    <>
      <div
        className="modal-backdrop cm-fade-in"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-flow-title"
        onMouseDown={(event) => {
          if (event.target === event.currentTarget) onClose();
        }}
      >
        <div className="modal-card flow-card cm-pop-in">
          <header className="modal-header">
            <span className="modal-header__icon">
              <Icon name="clock" />
            </span>
            <div>
              <h2 id="agent-flow-title">Flow 会话</h2>
              <p>按时间相邻、来源与共享关键词归组的短期会话</p>
            </div>
            <button type="button" onClick={onClose} aria-label="关闭" className="icon-button">
              <Icon name="close" />
            </button>
          </header>

          <div className="mt-3">
            {detail === null ? (
              <>
                {sessions === null && (
                  <p className="text-xs text-[var(--cm-fg-faint)]">读取中…</p>
                )}

                {sessions !== null && sessions.length === 0 && error === null && (
                  <p className="text-xs leading-relaxed text-[var(--cm-fg-muted)]">
                    {EMPTY_HINT}
                  </p>
                )}

                {sessions !== null && sessions.length > 0 && (
                  <ul className="flow-list scrollbar-thin">
                    {sessions.map((session) => (
                      <li key={session.id}>
                        <button
                          type="button"
                          onClick={() => void openSession(session.id)}
                          className="flex w-full flex-col items-start gap-0.5 rounded-[var(--cm-radius-sm)] px-1.5 py-1.5 text-left transition-colors hover:bg-[var(--cm-hover)]"
                        >
                          <span className="flex min-w-0 max-w-full items-center gap-1.5">
                            <strong className="truncate text-xs font-semibold text-[var(--cm-fg)]">
                              {session.title}
                            </strong>
                            {/* 「手动」只标用户亲手存的：自动算出来的会话有理由可查，
                                不需要这个标；手动存的那条没有理由，得有个别的记号。 */}
                            {session.source === "user" && (
                              <span className="flex-none rounded-[4px] bg-[var(--cm-accent-soft)] px-1 text-[9.5px] font-medium leading-[15px] text-[var(--cm-accent-text)]">
                                手动
                              </span>
                            )}
                          </span>
                          <span className="max-w-full truncate text-[10.5px] text-[var(--cm-fg-faint)]">
                            {session.summary}
                          </span>
                          <span className="tabular-nums text-[10px] text-[var(--cm-fg-faint)]">
                            {session.item_count} 条 · {formatSessionTime(session.updated_at)}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </>
            ) : (
              <>
                <button
                  type="button"
                  onClick={() => setDetail(null)}
                  className="-ml-1.5 inline-flex items-center rounded-[var(--cm-radius-sm)] px-1.5 py-1 text-[10.5px] text-[var(--cm-fg-muted)] transition-colors hover:bg-[var(--cm-hover)] hover:text-[var(--cm-fg)]"
                >
                  ← 返回列表
                </button>

                <h3 className="mt-1.5 text-xs font-semibold text-[var(--cm-fg)]">
                  {detail.title}
                </h3>
                <p className="mt-1 text-[10.5px] leading-relaxed text-[var(--cm-fg-faint)]">
                  {detail.summary}
                </p>

                <ul className="flow-list scrollbar-thin">
                  {detail.members.map((member) => (
                    <li key={member.item.id} className="flow-member">
                      {/* 编号直接用库里的 position（1-based）：界面上的号与库里的号
                          一致，省掉所有 +1 / -1 的换算（关键判断 12）。 */}
                      <span className="flex h-[18px] w-[18px] flex-none items-center justify-center rounded-full bg-[var(--cm-accent-soft)] text-[10px] font-semibold tabular-nums text-[var(--cm-accent-text)]">
                        {member.position}
                      </span>
                      <div className="min-w-0 flex-1">
                        <div className="flex min-w-0 items-center gap-1.5 text-[10.5px] text-[var(--cm-fg-muted)]">
                          <span className="truncate">{member.item.source_app ?? "未知来源"}</span>
                          <span className="meta-dot" />
                          <time className="tabular-nums">
                            {formatSessionTime(member.item.last_copied_at)}
                          </time>
                        </div>
                        <p className="mt-0.5 break-words text-[11.5px] leading-relaxed text-[var(--cm-fg)]">
                          {member.item.preview || "（图片）"}
                        </p>
                        {/* reason === null 是「用户手动保存」这一态，不是「理由缺失」。 */}
                        <p className="flow-reason">{member.reason ?? "手动保存"}</p>
                      </div>
                    </li>
                  ))}
                </ul>

                {/* 下一步建议：触发时机就是这个**显式按钮**（拍板 1），并且把
                    「这次会发生什么」写在按钮旁边 —— 这是用户对自己钱包的提示。 */}
                <div className="mt-2 border-t border-[var(--cm-line)] pt-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <button
                      type="button"
                      onClick={() =>
                        suggestions?.running === true
                          ? void handleCancelSuggest()
                          : void handleSuggest()
                      }
                      className="button button--secondary button--compact"
                    >
                      {suggestions?.running === true ? "取消" : "下一步建议"}
                    </button>
                    <span className="text-[10px] leading-[15px] text-[var(--cm-fg-faint)]">
                      会产生一次模型调用
                    </span>
                  </div>

                  {suggestions?.set != null && (
                    <>
                      {suggestions.set.suggestions.length === 0 ? (
                        // 空列表只有一种来源：模型明确说了「没有建议」。一行都没活下来
                        // 时后端返回的是 planner_suggestions_unusable 错误，走下面那条路。
                        <p className="mt-1.5 text-[10.5px] leading-relaxed text-[var(--cm-fg-faint)]">
                          这次没有值得执行的下一步
                        </p>
                      ) : (
                        <ul className="flow-list scrollbar-thin">
                          {suggestions.set.suggestions.map((suggestion) => (
                            <li key={suggestion.index} className="flow-member">
                              {/* 圆标 = 这次建议列表里的第几条（被丢弃的行不占号，
                                  关键判断 10），不是数组下标。 */}
                              <span className="flex h-[18px] w-[18px] flex-none items-center justify-center rounded-full bg-[var(--cm-accent-soft)] text-[10px] font-semibold tabular-nums text-[var(--cm-accent-text)]">
                                {suggestion.index}
                              </span>
                              <div className="min-w-0 flex-1">
                                <p className="break-words text-[11.5px] leading-relaxed text-[var(--cm-fg)]">
                                  {`${suggestionTitle(suggestion)} · ${targetSummary(suggestion.targets)}`}
                                </p>
                                {/* 理由原文：说不清理由的行走不到这里（后端就丢了）。 */}
                                <p className="flow-reason">{suggestion.reason}</p>
                              </div>
                              {isExecutablePlannerSuggestion(suggestion) ? (
                                <button
                                  type="button"
                                  onClick={() => handleRequestExecute(suggestion)}
                                  disabled={execution?.running === true}
                                  className="button button--secondary button--compact shrink-0 self-start"
                                >
                                  执行
                                </button>
                              ) : (
                                // 认不出来的动作类型连按钮都不给（前端第二道白名单）。
                                <span className="shrink-0 self-start text-[10px] leading-[18px] text-[var(--cm-fg-faint)]">
                                  当前版本不支持这个动作
                                </span>
                              )}
                            </li>
                          ))}
                        </ul>
                      )}

                      {suggestions.set.dropped > 0 && (
                        // 被丢弃的行如实告知（不静默吞掉）：N = 活下来的 + 被丢弃的。
                        <p className="mt-1.5 text-[10px] leading-[15px] text-[var(--cm-fg-faint)]">
                          {`模型给了 ${suggestions.set.suggestions.length + suggestions.set.dropped} 行，其中 ${suggestions.set.dropped} 行不符合协议已忽略`}
                        </p>
                      )}
                    </>
                  )}

                  {suggestions?.error != null && (
                    <p
                      role="alert"
                      className="mt-1.5 whitespace-pre-line text-[10.5px] leading-relaxed text-[var(--cm-danger)]"
                    >
                      ⚠ {suggestions.error}
                      {(suggestions.errorCode === "ai_not_configured" ||
                        suggestions.errorCode === "ai_invalid_base_url") && (
                        <button
                          type="button"
                          onClick={onOpenSettings}
                          className="agent-panel__link"
                        >
                          去设置
                        </button>
                      )}
                    </p>
                  )}

                  {execution?.running === true && (
                    <p className="mt-1.5 text-[10px] text-[var(--cm-fg-faint)]">执行中…</p>
                  )}

                  {execution?.error != null && (
                    <p
                      role="alert"
                      className="mt-1.5 text-[10.5px] leading-relaxed text-[var(--cm-danger)]"
                    >
                      ⚠ {execution.error}
                      {(execution.errorCode === "ai_not_configured" ||
                        execution.errorCode === "ai_invalid_base_url") && (
                        <button
                          type="button"
                          onClick={onOpenSettings}
                          className="agent-panel__link"
                        >
                          去设置
                        </button>
                      )}
                    </p>
                  )}

                  {/* AI 动作的草稿：结果只作为草稿显示在这里，原始记录一条不改。 */}
                  {execution?.result != null && (
                    <AgentResultCard
                      result={execution.result}
                      copied={execution.copied}
                      onCopy={() => void handleCopyActionResult()}
                    />
                  )}
                </div>
              </>
            )}

            {detailLoading && (
              <p className="mt-1.5 text-[10px] text-[var(--cm-fg-faint)]">读取中…</p>
            )}

            {notice && (
              <p
                role="status"
                aria-live="polite"
                className="mt-2 text-[10.5px] leading-relaxed text-[var(--cm-success)]"
              >
                {notice}
              </p>
            )}

            {error && (
              <p role="alert" className="mt-2 text-[10.5px] leading-relaxed text-[var(--cm-danger)]">
                ⚠ {error}
              </p>
            )}
          </div>

          <footer className="preview-foot">
            <span className="preview-foot__hint">
              会话由本地记录算出；只有「下一步建议」会产生一次模型调用 · <kbd>Esc</kbd> 关闭
            </span>
            <div className="preview-foot__actions">
              {detail !== null && (
                <button
                  type="button"
                  onClick={() => setConfirmDeleteOpen(true)}
                  disabled={deleting}
                  className="button button--danger"
                >
                  删除这个会话
                </button>
              )}
              <button type="button" onClick={onClose} className="button button--secondary">
                关闭
              </button>
            </div>
          </footer>
        </div>
      </div>

      <ConfirmDialog
        open={confirmDeleteOpen}
        title="删除这个会话"
        description="只删掉这一组分组结果，原始剪贴板记录一条都不会少。此操作不可撤销。"
        confirmLabel="删除"
        onConfirm={() => void handleDelete()}
        onCancel={() => setConfirmDeleteOpen(false)}
      />

      {/* 副作用确认卡片：确认前一条执行命令都不发。Esc 由 App 统一监听 —— 卡片
          开着时按 Esc 等于**放弃这次确认**（不执行），与既有删除确认的行为一致。 */}
      <ActionConfirmDialog
        open={execution?.pending != null}
        suggestion={execution?.pending ?? null}
        providerLabel={providerLabel}
        onConfirm={handleConfirmExecution}
        onCancel={() => {
          if (detail === null) return;
          updateExecution(detail.id, { pending: null });
        }}
      />
    </>
  );
}
