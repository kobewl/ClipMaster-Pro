import { useCallback, useEffect, useState } from "react";
import type { AgentSessionDetail, AgentSessionSummary } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import { ConfirmDialog } from "./ConfirmDialog";
import { Icon } from "./Icon";

interface Props {
  open: boolean;
  onClose: () => void;
}

/**
 * 空状态文案：解释**分组规则**，不是一句「暂无数据」。
 *
 * 用户看到空列表时最需要知道的是「怎么才有会话」，所以把三条硬条件里他能直接
 * 影响的两条（至少 2 条、时间相邻 ≤30 分钟）写出来。
 * 刻意不给「去设置」的路径：会话由本地字段算出、不调模型，空列表与 API Key 无关。
 */
const EMPTY_HINT =
  "最近 3 天里没有可以归组的内容 —— 会话至少需要 2 条时间相邻（≤30 分钟）且同源或共享关键词的记录。";

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
 * Flow 会话工作台：把时间相邻、说得清关系的片段看成一「段」。
 *
 * 单栏两级（列表 → 详情），详情里逐条给出**理由** —— 这是本功能与「聪明的自动
 * 关联」的分界线：凡是自动成组，每一条都必须能说出为什么（`reason`），说不清
 * 就不该成组。用户手动存的会话例外：他为什么放在一起不需要机器解释，那些成员
 * 显示「手动保存」。
 *
 * props 刻意**没有** `onOpenSettings`：会话不依赖 API Key，界面里不该出现一条
 * 通往设置的误导路径（与 AgentBatchDialog 的对比）。
 */
export function AgentFlowDialog({ open, onClose }: Props) {
  /** null = 还在读；[] = 读过且确实没有。这两种状态给的界面完全不同。 */
  const [sessions, setSessions] = useState<AgentSessionSummary[] | null>(null);
  const [detail, setDetail] = useState<AgentSessionDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

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
  useEffect(() => {
    if (!open) return;
    setDetail(null);
    setConfirmDeleteOpen(false);
    setError(null);
    setNotice(null);
    setSessions(null);
    void loadSessions();
  }, [open, loadSessions]);

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
              会话由本地记录算出，不调用模型 · <kbd>Esc</kbd> 关闭
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
    </>
  );
}
