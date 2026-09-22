import { useEffect, useMemo, useRef, useState } from "react";
import DOMPurify from "dompurify";
import { convertFileSrc } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { AgentAction, AgentResult, ClipboardItem } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import { createRequestId } from "@/lib/requestId";
import { extractDomain, getAppIcon } from "@/lib/sourceIcons";
import { AgentResultCard } from "./AgentResultCard";
import { Icon } from "./Icon";
import { ImageZoom } from "./ImageZoom";

interface Props {
  item: ClipboardItem | null;
  /** 来源应用的真实图标路径（null 时退回 emoji / 图片图标）。 */
  iconSrc: string | null;
  onCopy: (id: string) => void;
  onPaste: (id: string) => void;
  onClose: () => void;
  /** AI 未配置时，从结果区直接跳到设置面板里的 AI 助手。 */
  onOpenSettings: () => void;
}

function formatFullTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** 文件条目的 content_text 约定：绝对路径列表，用 \n 分隔（与后端一致）。 */
function splitFiles(filesText: string): string[] {
  return filesText.split("\n").map((path) => path.trim()).filter(Boolean);
}

function fileNameOf(path: string): string {
  const index = path.lastIndexOf("/");
  return index >= 0 ? path.slice(index + 1) : path;
}

function parentDirOf(path: string): string {
  const index = path.lastIndexOf("/");
  return index > 0 ? path.slice(0, index) : "";
}

/**
 * 取 HTML 的纯文本正文字数。走一次 DOM 解析（先消毒再量），
 * 不用正则数标签 —— 精确字符数对「共 N 字」这个口径是底线。
 */
function htmlPlainLength(html: string): number {
  const template = document.createElement("template");
  template.innerHTML = DOMPurify.sanitize(html);
  return (template.content.textContent ?? "").replace(/\s+/g, "").length;
}

/**
 * 「查看全部内容」弹窗。
 *
 * 列表里的正文只有两行，而且后端下发的 `preview` 字段在 240 字处就截断了；
 * 完整的 `content_text` 其实一直是传到前端的，只是界面里没地方显示它。
 * 这个弹窗就是那个地方 —— 顺带也解决了「图片只能看缩略图」的问题。
 */
export function PreviewDialog({ item, iconSrc, onCopy, onPaste, onClose, onOpenSettings }: Props) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const [agentResult, setAgentResult] = useState<AgentResult | null>(null);
  const [agentError, setAgentError] = useState<string | null>(null);
  /** 错误码单独记着：`ai_not_configured` 要在旁边给一个"去设置"的入口。 */
  const [agentErrorCode, setAgentErrorCode] = useState<string | null>(null);
  const [runningAction, setRunningAction] = useState<AgentAction | null>(null);
  const [copiedResult, setCopiedResult] = useState(false);
  /**
   * 在途请求的号（没有在途请求时为 null）。
   *
   * 两个用途，都依赖"号在发起时就已经在前端手里"这一点：
   * 1. 取消：按号定向取消，号对不上后端不会打断任何请求。
   * 2. 过期响应守卫：换了条目/关了弹窗之后，迟到回来的结果不能再渲染 ——
   *    否则上一条内容的结果会挂在新条目上，用户看到的是张冠李戴。
   */
  const activeRunRef = useRef<string | null>(null);
  /**
   * 当前配的服务名。面板标题原来硬编码 "DeepSeek"，用户把地址改成中转站
   * 或本地 Ollama 之后就成了假信息 —— 内容发去了哪里必须如实显示。
   */
  const [providerLabel, setProviderLabel] = useState<string>("DeepSeek");

  // 依赖 `item?.id` 而不是 `[]`：这个组件在 App 里是常驻挂载的，只跑一次的话，
  // 用户在同一次会话里改完服务地址、回来再看，右上角仍写着旧服务名。
  // 按条目刷新让"看哪条"与"发往哪里"两处始终同一次读取。
  useEffect(() => {
    commands
      .getAgentConfig()
      .then((info) => setProviderLabel(info.provider_label))
      .catch(() => {
        /* 读不到就保持默认文案，不影响 AI 操作本身（真正的错误会在点按钮时报出） */
      });
  }, [item?.id]);

  // 长文打开时从头开始看；不做滚动位置记忆，每次都是新的阅读。
  //
  // 依赖 `item?.id` 而不是 `item`：App.tsx 的 previewItem 是每次从刷新的 items
  // 数组里 find 出来的**同 id 新对象**，按对象身份依赖的话，用户在系统里复制
  // 任何东西（列表刷新）都会把正在看的结果和进行中的状态一起清掉。
  // 真正要重置的时机只有"换了另一条内容"（id 变）与"弹窗关掉"（id 变 undefined）。
  useEffect(() => {
    if (item) bodyRef.current?.scrollTo({ top: 0 });
    setAgentResult(null);
    setAgentError(null);
    setAgentErrorCode(null);
    setRunningAction(null);
    setCopiedResult(false);
  }, [item?.id]);

  /**
   * 离开这条内容（换条目、关弹窗、卸载）时，把还在飞的请求取消掉。两条理由：
   * 用户已经不打算看这次结果了 —— 让它在后台跑完等于白计费；而结果真回来
   * 也没地方显示，只会变成串台隐患。
   *
   * 依赖同样是 `item?.id`（理由见上面的状态重置 effect）：**同一条目的列表刷新
   * 不是"离开"**，绝不能因此取消在途请求 —— 那会在审计里留下假的 ai_cancelled。
   *
   * 先清 ref 再发取消：清理函数可能因为 id 切换而再次进入，ref 空了就不会
   * 重复发一轮取消。
   */
  useEffect(() => {
    return () => {
      const requestId = activeRunRef.current;
      if (!requestId) return;
      activeRunRef.current = null;
      // 取消失败不致命：请求会自然结束，过期响应守卫会挡住它的结果。
      void commands.cancelAgentAction(requestId).catch(() => {});
    };
  }, [item?.id]);

  const meta = useMemo(() => {
    if (!item) return null;
    const isImage = item.content_type === "image";
    const isHtml = item.content_type === "html";
    const isFiles = item.content_type === "files";
    // 字数只对文本有意义；图片报「图片」，文件报个数，HTML 报去标签后的正文字数。
    const size = isImage
      ? "图片"
      : isFiles
        ? `${splitFiles(item.content_text).length} 个文件`
        : isHtml
          ? `共 ${htmlPlainLength(item.content_text)} 字`
          : `共 ${item.content_text.length} 字`;
    return {
      isImage,
      isHtml,
      isFiles,
      source: extractDomain(item.source_url) ?? item.source_app ?? "未知来源",
      time: formatFullTime(item.last_copied_at),
      size,
    };
  }, [item]);

  /**
   * 富文本**只在展示这一刻**消毒（DOMPurify 默认白名单，脚本/事件属性全剥掉）。
   * 数据库里的 content_text 永远是原文 —— 回写剪贴板时粘出去的仍是带格式的内容，
   * 消毒只对「渲染进界面」这一步负责，安全边界清晰。
   */
  const sanitizedHtml = useMemo(
    () => (meta?.isHtml ? DOMPurify.sanitize(item?.content_text ?? "") : null),
    [meta?.isHtml, item],
  );

  const filePaths = useMemo(
    () => (meta?.isFiles && item ? splitFiles(item.content_text) : []),
    [meta?.isFiles, item],
  );

  async function handleReveal(path: string) {
    try {
      await revealItemInDir(path);
    } catch {
      // 打不开 Finder 不致命：路径本身在界面上可见，用户能自己去找。
    }
  }

  async function handleAgentAction(action: AgentAction) {
    if (!item || runningAction) return;
    const requestId = createRequestId();
    activeRunRef.current = requestId;
    setRunningAction(action);
    setAgentError(null);
    setAgentErrorCode(null);
    setCopiedResult(false);
    try {
      const result = await commands.runAgentAction(item.id, action, requestId);
      // 过期响应守卫：换了条目（或已经取消）之后回来的结果不许渲染。
      if (activeRunRef.current !== requestId) return;
      setAgentResult(result);
    } catch (error) {
      // 失败路径同样要守卫：否则上一条内容的报错会挂在新条目上。
      if (activeRunRef.current !== requestId) return;
      if (isCommandError(error)) {
        setAgentError(error.message);
        setAgentErrorCode(error.code);
      } else {
        setAgentError("AI 操作失败，请稍后重试。");
      }
    } finally {
      // 只有"还是这次请求"才收尾；否则说明它已经过期，状态归零由新条目负责。
      if (activeRunRef.current === requestId) {
        activeRunRef.current = null;
        setRunningAction(null);
      }
    }
  }

  async function handleCopyAgentResult() {
    if (!agentResult) return;
    try {
      await commands.copyTextToClipboard(agentResult.content);
      setCopiedResult(true);
    } catch {
      setAgentError("复制 AI 结果失败。");
    }
  }

  /**
   * 取消进行中的请求。带上在途请求的号 —— 后端只取消号匹配的那一次。
   * 后端会把那次请求以 `ai_cancelled` 结束，所以这里不用自己造错误文案：
   * 正常错误路径会把「已取消本次请求。」显示出来（ref 里仍是这个号，
   * 守卫不会把它挡掉）。
   */
  async function handleCancelAction() {
    const requestId = activeRunRef.current;
    if (!requestId) return;
    try {
      await commands.cancelAgentAction(requestId);
    } catch {
      // 取消失败不致命：请求会自然结束，错误路径会正常显示
    }
  }

  if (!item || !meta) return null;

  const supportsAgent = !meta.isImage && !meta.isFiles;
  const agentActions: Array<{ action: AgentAction; label: string }> = [
    { action: "summarize", label: "总结" },
    { action: "translate_zh", label: "翻译" },
    { action: "explain", label: "解释" },
    { action: "extract_tasks", label: "提取待办" },
    { action: "format_json", label: "格式化 JSON" },
  ];

  return (
    <div
      className="modal-backdrop cm-fade-in"
      role="dialog"
      aria-modal="true"
      aria-labelledby="preview-dialog-title"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className={`modal-card preview-card ${meta.isImage ? "preview-card--image" : ""} cm-pop-in`}
      >
        <header className="modal-header">
          <span className="modal-header__icon">
            {/* 真实图标 > 图片图标 > emoji，与列表行保持同一套兜底顺序 */}
            {iconSrc ? (
              <img src={convertFileSrc(iconSrc)} alt="" className="preview-card__appicon" />
            ) : meta.isImage ? (
              <Icon name="image" />
            ) : (
              <span>{getAppIcon(item.source_app)}</span>
            )}
          </span>
          <div>
            <h2 id="preview-dialog-title">{meta.source}</h2>
            <p>
              {meta.time} · {meta.size}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭"
            className="icon-button"
          >
            <Icon name="close" />
          </button>
        </header>

        {meta.isImage ? (
          <div className="preview-image-wrap">
            <ImageZoom src={convertFileSrc(item.content_text)} alt="剪贴板图片" />
          </div>
        ) : meta.isFiles ? (
          <div ref={bodyRef} className="preview-body scrollbar-thin">
            <ul className="file-list">
              {filePaths.map((path) => (
                <li key={path} className="file-list__row">
                  <span className="file-list__icon" aria-hidden>📄</span>
                  <div className="file-list__text">
                    <span className="file-list__name">{fileNameOf(path)}</span>
                    <span className="file-list__dir">{parentDirOf(path)}</span>
                  </div>
                  <button
                    type="button"
                    onClick={() => void handleReveal(path)}
                    className="button button--secondary button--compact"
                    title="在 Finder 中显示该文件"
                  >
                    显示
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ) : meta.isHtml ? (
          <div ref={bodyRef} className="preview-body scrollbar-thin">
            <div
              className="preview-body__html"
              // sanitizedHtml 已过 DOMPurify 白名单；渲染原文在这里永远不出现。
              dangerouslySetInnerHTML={{ __html: sanitizedHtml ?? "" }}
            />
          </div>
        ) : (
          <div ref={bodyRef} className="preview-body scrollbar-thin">
            <p className="preview-body__text">{item.content_text}</p>
          </div>
        )}

        <section className="agent-panel" aria-label="ClipMaster Agent">
          <div className="agent-panel__head">
            <div>
              <strong>✨ ClipMaster Agent</strong>
              <span>结果是草稿，原始剪贴板内容不会被修改</span>
            </div>
            <span className="agent-panel__model">{providerLabel}</span>
          </div>
          {supportsAgent ? (
            <div className="agent-panel__actions">
              {agentActions.map(({ action, label }) => (
                <button
                  type="button"
                  key={action}
                  className="agent-action"
                  // 进行中的那个按钮要留给用户按「取消」，所以它自己不能是禁用的；
                  // 其余按钮在请求期间照旧全部禁用，避免连点发出第二个请求。
                  disabled={runningAction !== null && runningAction !== action}
                  onClick={() =>
                    runningAction === action
                      ? void handleCancelAction()
                      : void handleAgentAction(action)
                  }
                >
                  {runningAction === action ? "取消" : label}
                </button>
              ))}
            </div>
          ) : (
            <p className="agent-panel__notice">图片和文件理解将在后续版本以明确授权的方式提供。</p>
          )}
          {agentError && (
            <p className="agent-panel__error" role="alert">
              {agentError}
              {agentErrorCode === "ai_not_configured" && (
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
          {agentResult && (
            <AgentResultCard
              result={agentResult}
              copied={copiedResult}
              onCopy={() => void handleCopyAgentResult()}
            />
          )}
        </section>

        <footer className="preview-foot">
          <span className="preview-foot__hint">
            {meta.isImage
              ? "点图片可放大 · ⌘ 滚轮缩放"
              : meta.isHtml
                ? "富文本预览（消毒后渲染）· 可拖选"
                : meta.isFiles
                  ? "点「显示」在 Finder 中定位文件"
                  : "可以直接拖选文字"}{" "}
            · <kbd>Esc</kbd> 关闭
          </span>
          <div className="preview-foot__actions">
            <button
              type="button"
              onClick={() => onCopy(item.id)}
              className="button button--secondary"
            >
              复制
            </button>
            <button
              type="button"
              onClick={() => onPaste(item.id)}
              autoFocus
              className="button button--primary"
            >
              粘贴
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
