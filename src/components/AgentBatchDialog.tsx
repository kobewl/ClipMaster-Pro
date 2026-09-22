import { useEffect, useRef, useState } from "react";
import type { AgentAction, AgentResult, ClipboardItem } from "@/types/clipboard";
import { MAX_AGENT_INPUT_ITEMS, isCommandError, supportsBatch } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import { createRequestId } from "@/lib/requestId";
import { AgentResultCard } from "./AgentResultCard";
import { Icon } from "./Icon";

interface Props {
  open: boolean;
  /** 当前勾选的条目（顺序无关，后端会按时间重排并编号）。 */
  items: ClipboardItem[];
  onClose: () => void;
  /** AI 未配置时，从结果区直接跳到设置面板。 */
  onOpenSettings: () => void;
}

/**
 * 多条目 AI 工作台：把勾选的一批记录交给模型做跨记录归纳。
 *
 * 与预览弹窗里的单条动作分开，是因为两者的**关注点不同**：单条是"读懂这一条"，
 * 这里是"读懂它们之间的关系"。结果里逐条列出每条的实际处理情况（编号、来源、
 * 是否被截断），用户才能核对结论是哪几条支撑的 —— 这是多条目上下文最容易
 * 出问题的地方：模型给出一段听起来合理的归纳，实际只看到了其中三条。
 */
export function AgentBatchDialog({ open, items, onClose, onOpenSettings }: Props) {
  const [result, setResult] = useState<AgentResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const [running, setRunning] = useState<AgentAction | null>(null);
  const [copied, setCopied] = useState(false);
  /**
   * 在途请求的号（没有在途请求时为 null）。与 PreviewDialog 同一套模式：
   * 取消按号定向，过期响应守卫挡住关窗后迟到的结果。
   */
  const activeRunRef = useRef<string | null>(null);

  // 每次打开都从干净状态开始；关掉再打开不该看到上一次的结果。
  useEffect(() => {
    if (!open) return;
    setResult(null);
    setError(null);
    setErrorCode(null);
    setRunning(null);
    setCopied(false);
  }, [open]);

  /**
   * 面板关掉（open 变 false）或卸载时，取消还在飞的请求：用户已经不看结果了，
   * 让它在后台跑完等于白花一次钱。`if (!open) return;` 放在 effect 体开头 ——
   * 这样清理函数只在面板**曾经打开**过的那一轮注册，关窗和卸载两条路径都能走到。
   */
  useEffect(() => {
    if (!open) return;
    return () => {
      const requestId = activeRunRef.current;
      if (!requestId) return;
      activeRunRef.current = null;
      // 取消失败不致命：请求会自然结束，过期响应守卫会挡住它的结果。
      void commands.cancelAgentAction(requestId).catch(() => {});
    };
  }, [open]);

  if (!open) return null;

  const overLimit = items.length > MAX_AGENT_INPUT_ITEMS;
  const actions: Array<{ action: AgentAction; label: string }> = [
    { action: "summarize", label: "归纳总结" },
    { action: "translate_zh", label: "翻译为中文" },
    { action: "explain", label: "解释这一组" },
    { action: "extract_tasks", label: "汇总待办" },
  ];

  async function handleRun(action: AgentAction) {
    if (running) return;
    const requestId = createRequestId();
    activeRunRef.current = requestId;
    setRunning(action);
    setError(null);
    setErrorCode(null);
    setCopied(false);
    // 清掉上一次的结果：留在屏幕上会挂着新动作的标题、内容是上一个动作的，
    // 用户看到的就是一次张冠李戴。
    setResult(null);
    try {
      const ids = items.map((item) => item.id);
      const result = await commands.runAgentActionBatch(ids, action, requestId);
      // 过期响应守卫：面板已经关掉（或已经取消）之后回来的结果不许渲染。
      if (activeRunRef.current !== requestId) return;
      setResult(result);
    } catch (err: unknown) {
      if (activeRunRef.current !== requestId) return;
      if (isCommandError(err)) {
        setError(err.message);
        setErrorCode(err.code);
      } else {
        setError("AI 操作失败，请稍后重试。");
      }
    } finally {
      // 只有"还是这次请求"才收尾；过期的请求由关窗那一轮负责清状态。
      if (activeRunRef.current === requestId) {
        activeRunRef.current = null;
        setRunning(null);
      }
    }
  }

  async function handleCopy() {
    if (!result) return;
    try {
      await commands.copyTextToClipboard(result.content);
      setCopied(true);
    } catch {
      setError("复制 AI 结果失败。");
    }
  }

  /**
   * 取消进行中的请求。带号取消：后端只取消号匹配的那一次。
   * 取消后后端让那次请求以 `ai_cancelled` 结束，「已取消本次请求。」
   * 会走正常错误路径显示出来。
   */
  async function handleCancel() {
    const requestId = activeRunRef.current;
    if (!requestId) return;
    try {
      await commands.cancelAgentAction(requestId);
    } catch {
      // 取消失败不致命：请求会自然结束，错误路径会正常显示
    }
  }

  return (
    <div
      className="modal-backdrop cm-fade-in"
      role="dialog"
      aria-modal="true"
      aria-labelledby="agent-batch-title"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal-card cm-pop-in agent-batch-card">
        <header className="modal-header">
          <span className="modal-header__icon">
            <Icon name="check" />
          </span>
          <div>
            <h2 id="agent-batch-title">AI 归纳</h2>
            <p>已选 {items.length} 条记录</p>
          </div>
          <button type="button" onClick={onClose} aria-label="关闭" className="icon-button">
            <Icon name="close" />
          </button>
        </header>

        <div className="agent-batch-body scrollbar-thin">
          {overLimit && (
            <p className="agent-batch__notice" role="alert">
              一次最多处理 {MAX_AGENT_INPUT_ITEMS} 条，当前选中 {items.length} 条。
              请缩小范围后重试 —— 先筛出真正相关的那几条，归纳质量也会更好。
            </p>
          )}

          <div className="agent-panel__actions">
            {actions.map(({ action, label }) => (
              <button
                type="button"
                key={action}
                className="agent-action"
                // 进行中的那个按钮同时是「取消」入口，所以它不能在请求期间被禁用；
                // 其余按钮的禁用规则不变（跑着、超限、不支持批量）。
                disabled={
                  running !== action &&
                  (running !== null || overLimit || !supportsBatch(action))
                }
                onClick={() =>
                  running === action ? void handleCancel() : void handleRun(action)
                }
              >
                {running === action ? "取消" : label}
              </button>
            ))}
          </div>

          <p className="agent-batch__hint">
            模型只看到这些记录的内容本身；结果里的 <code>[1] [2]</code> 对应下面清单里的编号。
          </p>

          {error && (
            <p className="agent-panel__error" role="alert">
              {error}
              {errorCode === "ai_not_configured" && (
                <button
                  type="button"
                  className="agent-panel__link"
                  onClick={() => {
                    onClose();
                    onOpenSettings();
                  }}
                >
                  去设置
                </button>
              )}
            </p>
          )}

          {result && (
            <AgentResultCard result={result} copied={copied} onCopy={() => void handleCopy()} />
          )}

          {!result && !error && (
            <ul className="agent-batch__preview">
              {items.slice(0, 8).map((item) => (
                <li key={item.id} className="agent-batch__preview-row">
                  <span className="agent-batch__preview-source">
                    {item.source_app ?? "未知来源"}
                  </span>
                  <span className="agent-batch__preview-text">
                    {item.preview || item.content_text.slice(0, 80) || "（图片）"}
                  </span>
                </li>
              ))}
              {items.length > 8 && (
                <li className="agent-batch__preview-more">…还有 {items.length - 8} 条</li>
              )}
            </ul>
          )}
        </div>

        <footer className="preview-foot">
          <span className="preview-foot__hint">
            结果只是草稿，不会改动原始记录 · <kbd>Esc</kbd> 关闭
          </span>
          <div className="preview-foot__actions">
            <button type="button" onClick={onClose} className="button button--secondary">
              关闭
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
