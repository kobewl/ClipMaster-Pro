import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AgentChatDetail, AgentChatMessage, AgentChatSummary } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import { onAgentChatDelta, onAgentChatStarted, onAgentChatTool } from "@/lib/events";
import { createRequestId } from "@/lib/requestId";
import { ConfirmDialog } from "./ConfirmDialog";
import { Icon } from "./Icon";

export interface AgentChatSeed {
  itemIds?: string[];
  searchHint?: string;
  /** 从预览 / 多选进来时开新对话，不要接着上一段聊。 */
  startFresh?: boolean;
}

interface Props {
  open: boolean;
  seed: AgentChatSeed | null;
  onClose: () => void;
  onOpenSettings: () => void;
}

const MAX_USER_CHARS = 4000;

const STARTERS = [
  { label: "最近复制了什么？", message: "我最近复制了哪些内容？按时间归纳一下。" },
  { label: "帮我找昨天的命令", message: "帮我找昨天复制过的命令或报错。" },
  { label: "归纳最近的排查", message: "把最近的排查、报错和结论归纳成一段交接说明。" },
];

function chatErrorMessage(error: unknown): { message: string; code: string | null } {
  if (isCommandError(error)) {
    if (error.code === "ai_busy") {
      return { message: "正在处理上一次请求。等它完成，或点「取消」后再问。", code: error.code };
    }
    return { message: error.message, code: error.code };
  }
  return { message: "这次没问成，请稍后再试。", code: null };
}

function formatChatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString(undefined, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

/**
 * 自然语言对话工作台。
 *
 * 越用越准靠的是同一段对话里的上文 + 只读搜索工具，不是再堆一批动作按钮。
 * 正文是派生数据，设置里可以一键抹掉；工具调用不落库。
 */
export function AgentChatDialog({ open, seed, onClose, onOpenSettings }: Props) {
  const [chats, setChats] = useState<AgentChatSummary[]>([]);
  const [detail, setDetail] = useState<AgentChatDetail | null>(null);
  const [draft, setDraft] = useState("");
  const [streaming, setStreaming] = useState("");
  const [toolHint, setToolHint] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const [providerLabel, setProviderLabel] = useState("DeepSeek");
  const [configured, setConfigured] = useState<boolean | null>(null);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [itemIds, setItemIds] = useState<string[]>([]);
  const [searchHint, setSearchHint] = useState("");

  const activeRunRef = useRef<string | null>(null);
  const threadRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const loadList = useCallback(async () => {
    try {
      setChats(await commands.listAgentChats());
    } catch {
      setChats([]);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    setError(null);
    setErrorCode(null);
    setStreaming("");
    setToolHint(null);
    setRunning(false);
    setCopied(false);
    setConfirmDeleteOpen(false);
    setDraft("");
    setItemIds(seed?.itemIds ?? []);
    setSearchHint(seed?.searchHint?.trim() || "");
    void commands
      .getAgentConfig()
      .then((info) => {
        setProviderLabel(info.provider_label);
        setConfigured(info.configured);
      })
      .catch(() => {
        setConfigured(null);
      });
    void (async () => {
      await loadList();
      if (seed?.startFresh) {
        setDetail(null);
        return;
      }
      const listed = await commands.listAgentChats().catch(() => [] as AgentChatSummary[]);
      if (listed[0]) {
        const next = await commands.getAgentChat(listed[0].id).catch(() => null);
        setDetail(next);
      } else {
        setDetail(null);
      }
    })();
  }, [open, seed?.startFresh, seed?.itemIds?.join(","), seed?.searchHint, loadList]);

  useEffect(() => {
    if (!open) return;
    const unlistenStarted = onAgentChatStarted((payload) => {
      if (payload.request_id !== activeRunRef.current) return;
      setDetail((prev) =>
        prev && !prev.id ? { ...prev, id: payload.conversation_id } : prev,
      );
    });
    const unlistenDelta = onAgentChatDelta((payload) => {
      if (payload.request_id !== activeRunRef.current) return;
      setStreaming((prev) => prev + payload.text);
    });
    const unlistenTool = onAgentChatTool((payload) => {
      if (payload.request_id !== activeRunRef.current) return;
      if (payload.phase === "start") {
        setToolHint(
          payload.name === "search_clipboard"
            ? payload.detail
              ? `正在搜索「${payload.detail}」…`
              : "正在搜索剪贴板…"
            : "正在阅读剪贴板记录…",
        );
      } else {
        setToolHint(null);
      }
    });
    return () => {
      void unlistenStarted.then((stop) => stop());
      void unlistenDelta.then((stop) => stop());
      void unlistenTool.then((stop) => stop());
    };
  }, [open]);

  useEffect(() => {
    return () => {
      const requestId = activeRunRef.current;
      if (!requestId) return;
      activeRunRef.current = null;
      void commands.cancelAgentAction(requestId).catch(() => {});
    };
  }, [open]);

  useEffect(() => {
    threadRef.current?.scrollTo({ top: threadRef.current.scrollHeight });
  }, [detail?.messages.length, streaming, toolHint, running]);

  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open, detail?.id]);

  const messages = detail?.messages ?? [];
  const lastAssistant = [...messages].reverse().find((item) => item.role === "assistant");
  const draftCount = [...draft].length;
  const sendDisabled = running || draft.trim().length === 0 || draftCount > MAX_USER_CHARS;
  const starters = useMemo(() => {
    const extra = [];
    if (searchHint) {
      extra.push({
        label: `和「${searchHint}」相关的记录`,
        message: `帮我找和「${searchHint}」有关的剪贴板记录，并说明它们在说什么。`,
      });
    }
    if (itemIds.length > 0) {
      extra.push({
        label: itemIds.length === 1 ? "这条是什么意思？" : "这几条是在说什么？",
        message: itemIds.length === 1 ? "这条剪贴板记录是什么意思？帮我讲清楚。" : "把这几条剪贴板记录放在一起看，它们在说什么？",
      });
    }
    return [...extra, ...STARTERS].slice(0, 4);
  }, [itemIds.length, searchHint]);

  async function openChat(id: string) {
    if (running) return;
    setError(null);
    setErrorCode(null);
    setStreaming("");
    const next = await commands.getAgentChat(id).catch(() => null);
    setDetail(next);
    setItemIds([]);
    setSearchHint("");
  }

  function startNew() {
    if (running) return;
    setDetail(null);
    setStreaming("");
    setError(null);
    setErrorCode(null);
    setDraft("");
    inputRef.current?.focus();
  }

  async function send(text: string) {
    const message = text.trim();
    if (!message || running) return;
    if ([...message].length > MAX_USER_CHARS) {
      setError(`最多 ${MAX_USER_CHARS} 字。`);
      setErrorCode("ai_input_too_long");
      return;
    }
    const requestId = createRequestId();
    activeRunRef.current = requestId;
    setRunning(true);
    setError(null);
    setErrorCode(null);
    setStreaming("");
    setToolHint(null);
    setCopied(false);
    setDraft("");
    const optimistic: AgentChatMessage = {
      role: "user",
      content: message,
      created_at: new Date().toISOString(),
    };
    setDetail((prev) =>
      prev
        ? { ...prev, messages: [...prev.messages, optimistic] }
        : {
            id: "",
            title: message,
            created_at: optimistic.created_at,
            updated_at: optimistic.created_at,
            messages: [optimistic],
          },
    );
    try {
      const next = await commands.sendAgentChat({
        conversationId: detail?.id || null,
        message,
        itemIds,
        searchHint: searchHint || null,
        requestId,
      });
      if (activeRunRef.current !== requestId) return;
      setDetail(next);
      setStreaming("");
      await loadList();
    } catch (err: unknown) {
      if (activeRunRef.current !== requestId) return;
      const mapped = chatErrorMessage(err);
      setError(mapped.message);
      setErrorCode(mapped.code);
      setStreaming("");
    } finally {
      if (activeRunRef.current === requestId) {
        activeRunRef.current = null;
        setRunning(false);
        setToolHint(null);
      }
    }
  }

  async function handleCancel() {
    const requestId = activeRunRef.current;
    if (!requestId) return;
    try {
      await commands.cancelAgentAction(requestId);
    } catch {
      /* 取消失败不致命：请求会自然结束 */
    }
  }

  async function handleDelete() {
    if (!detail?.id) {
      setConfirmDeleteOpen(false);
      startNew();
      return;
    }
    try {
      await commands.deleteAgentChat(detail.id);
      setConfirmDeleteOpen(false);
      setDetail(null);
      await loadList();
    } catch (err: unknown) {
      setError(isCommandError(err) ? err.message : "删除对话失败");
    }
  }

  async function handleCopyLast() {
    const text = lastAssistant?.content;
    if (!text) return;
    try {
      await commands.copyTextToClipboard(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setError("复制失败");
    }
  }

  if (!open) return null;

  return (
    <div
      className="modal-backdrop cm-fade-in"
      role="dialog"
      aria-modal="true"
      aria-labelledby="agent-chat-title"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !running) onClose();
      }}
    >
      <div className="modal-card chat-card cm-pop-in">
        <header className="modal-header">
          <span className="modal-header__icon">
            <Icon name="sparkles" />
          </span>
          <div>
            <h2 id="agent-chat-title">问 AI</h2>
            <p>
              {providerLabel} · 只读搜索剪贴板，不会改你的记录
            </p>
          </div>
          <button type="button" onClick={onClose} aria-label="关闭" className="icon-button">
            <Icon name="close" />
          </button>
        </header>

        <div className="chat-layout">
          <aside className="chat-side" aria-label="最近对话">
            <button type="button" className="chat-side__new" onClick={startNew} disabled={running}>
              <Icon name="plus" />
              新对话
            </button>
            {chats.length === 0 ? (
              <p className="chat-side__empty">还没有对话</p>
            ) : (
              <ul className="chat-side__list">
                {chats.map((chat) => (
                  <li key={chat.id}>
                    <button
                      type="button"
                      className={`chat-side__item ${detail?.id === chat.id ? "chat-side__item--active" : ""}`}
                      onClick={() => void openChat(chat.id)}
                      disabled={running}
                    >
                      <strong>{chat.title}</strong>
                      <span>
                        {chat.message_count} 句 · {formatChatTime(chat.updated_at)}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </aside>

          <div className="chat-main">
            <div ref={threadRef} className="chat-thread scrollbar-thin">
              {messages.length === 0 && !running ? (
                <div className="chat-empty">
                  <strong>问你剪贴板里的任何东西</strong>
                  <p>我会搜索、阅读，再回答。同一段对话里接着问，会越来越懂你在忙什么。</p>
                  <div className="chat-starters">
                    {starters.map((item) => (
                      <button
                        key={item.label}
                        type="button"
                        className="chat-starter"
                        onClick={() => void send(item.message)}
                        disabled={configured === false}
                      >
                        {item.label}
                      </button>
                    ))}
                  </div>
                </div>
              ) : (
                messages.map((item, index) => (
                  <article
                    key={`${item.created_at}-${index}`}
                    className={`chat-bubble chat-bubble--${item.role === "user" ? "user" : "assistant"}`}
                  >
                    <p>{item.content}</p>
                  </article>
                ))
              )}
              {running && streaming && (
                <article className="chat-bubble chat-bubble--assistant">
                  <p>{streaming}</p>
                </article>
              )}
              {running && !streaming && (
                <p className="chat-status">{toolHint ?? "正在想…"}</p>
              )}
              {running && streaming && toolHint && <p className="chat-status">{toolHint}</p>}
            </div>

            {(itemIds.length > 0 || searchHint) && (
              <div className="chat-chips">
                {itemIds.length > 0 && (
                  <span className="chat-chip">已附带 {itemIds.length} 条剪贴板</span>
                )}
                {searchHint && <span className="chat-chip">当前搜索：{searchHint}</span>}
              </div>
            )}

            {error && (
              <p className="chat-error" role="alert">
                {error}
                {(errorCode === "ai_not_configured" || errorCode === "ai_invalid_base_url") && (
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

            {configured === false && (
              <p className="chat-error" role="status">
                还没有配置 API Key。
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
              </p>
            )}

            <div className="chat-composer">
              <textarea
                ref={inputRef}
                value={draft}
                rows={2}
                placeholder={configured === false ? "先到设置里填入 API Key" : "问剪贴板里的内容…"}
                disabled={running || configured === false}
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !event.shiftKey) {
                    event.preventDefault();
                    void send(draft);
                  }
                }}
              />
              <div className="chat-composer__bar">
                <span className="chat-composer__hint">
                  {draftCount > 3500 ? `${draftCount}/${MAX_USER_CHARS}` : "Enter 发送 · Shift+Enter 换行"}
                </span>
                <div className="chat-composer__actions">
                  {detail?.id && (
                    <button
                      type="button"
                      className="button button--secondary button--compact"
                      onClick={() => setConfirmDeleteOpen(true)}
                      disabled={running}
                    >
                      删除这段
                    </button>
                  )}
                  {lastAssistant && !running && (
                    <button
                      type="button"
                      className="button button--secondary button--compact"
                      onClick={() => void handleCopyLast()}
                    >
                      {copied ? "已复制" : "复制回答"}
                    </button>
                  )}
                  {running ? (
                    <button
                      type="button"
                      className="button button--secondary button--compact"
                      onClick={() => void handleCancel()}
                    >
                      取消
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="button button--primary button--compact"
                      disabled={sendDisabled || configured === false}
                      onClick={() => void send(draft)}
                    >
                      发送
                    </button>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <ConfirmDialog
        open={confirmDeleteOpen}
        title="删除这段对话？"
        description="只删对话记录，剪贴板历史不受影响。"
        confirmLabel="删除"
        onConfirm={() => void handleDelete()}
        onCancel={() => setConfirmDeleteOpen(false)}
      />
    </div>
  );
}
