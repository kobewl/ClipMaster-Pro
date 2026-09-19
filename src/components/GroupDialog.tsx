import { useState } from "react";
import type { ClipGroup } from "@/types/clipboard";
import { GROUP_COLORS, isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import { Icon } from "./Icon";

interface GroupDialogProps {
  groups: ClipGroup[];
  onClose: () => void;
  onChanged: () => void;
}

export function GroupDialog({ groups, onClose, onChanged }: GroupDialogProps) {
  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState<string>(GROUP_COLORS[5]);
  const [editId, setEditId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [editColor, setEditColor] = useState("");
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** 所有写操作都收口在这里：统一处理"按钮禁用 + 错误提示"。 */
  async function run(action: () => Promise<unknown>, fallbackMessage: string) {
    setBusy(true);
    setError(null);
    try {
      await action();
      onChanged();
    } catch (err: unknown) {
      setError(isCommandError(err) ? err.message : fallbackMessage);
    } finally {
      setBusy(false);
    }
  }

  async function handleCreate() {
    const name = newName.trim();
    if (!name) return;
    await run(async () => {
      await commands.createGroup({ name, color: newColor });
      setNewName("");
    }, "创建分组失败");
  }

  async function handleUpdate(id: string) {
    const name = editName.trim();
    if (!name) return;
    await run(async () => {
      await commands.updateGroup({ id, name, color: editColor });
      setEditId(null);
    }, "保存分组失败");
  }

  async function handleDelete(id: string) {
    await run(async () => {
      await commands.deleteGroup(id);
      setConfirmDeleteId(null);
    }, "删除分组失败");
  }

  return (
    <div
      className="modal-backdrop cm-fade-in"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal-card group-card cm-pop-in">
        <div className="modal-header">
          <span className="modal-header__icon"><Icon name="folder" /></span>
          <div><h2>管理分组</h2><p>用颜色和名称整理常用内容</p></div>
          <button className="icon-button" onClick={onClose} aria-label="关闭分组管理"><Icon name="close" /></button>
        </div>

        <div className="scrollbar-thin mt-3 max-h-[240px] space-y-1.5 overflow-y-auto">
          {groups.map((group) => {
            const editing = editId === group.id;
            const confirming = confirmDeleteId === group.id;

            return (
              <div
                key={group.id}
                className="rounded-lg bg-black/[0.02] px-2 py-1.5 dark:bg-white/[0.04]"
              >
                <div className="flex items-center gap-2">
                  <span
                    className="h-3 w-3 shrink-0 rounded-full"
                    style={{ backgroundColor: editing ? editColor : group.color }}
                  />
                  {editing ? (
                    <>
                      <input
                        type="text"
                        value={editName}
                        onChange={(event) => setEditName(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") handleUpdate(group.id);
                          if (event.key === "Escape") setEditId(null);
                        }}
                        className="min-w-0 flex-1 rounded border border-[var(--cm-line)] bg-transparent px-1.5 py-0.5 text-xs outline-none focus:border-[var(--cm-accent-ring)] dark:border-white/10"
                        autoFocus
                      />
                      <button
                        type="button"
                        onClick={() => handleUpdate(group.id)}
                        disabled={busy || !editName.trim()}
                        className="text-[10px] text-[var(--cm-accent-text)] hover:text-blue-600 disabled:opacity-40"
                      >
                        保存
                      </button>
                      <button
                        type="button"
                        onClick={() => setEditId(null)}
                        className="text-[10px] text-[var(--cm-fg-faint)] hover:text-[var(--cm-fg-muted)]"
                      >
                        取消
                      </button>
                    </>
                  ) : (
                    <>
                      <span className="min-w-0 flex-1 truncate text-xs text-[var(--cm-fg)]">
                        {group.name}
                      </span>
                      <span className="text-[10px] tabular-nums text-[var(--cm-fg-faint)]">
                        {group.item_count}
                      </span>
                      <button
                        type="button"
                        onClick={() => {
                          setConfirmDeleteId(null);
                          setEditId(group.id);
                          setEditName(group.name);
                          setEditColor(group.color);
                        }}
                        className="text-[10px] text-[var(--cm-fg-faint)] transition-colors hover:text-[var(--cm-fg-muted)]"
                      >
                        编辑
                      </button>
                      <button
                        type="button"
                        onClick={() => setConfirmDeleteId(confirming ? null : group.id)}
                        className={`text-[10px] transition-colors ${
                          confirming ? "text-red-600" : "text-[var(--cm-danger)] hover:text-red-600"
                        }`}
                      >
                        删除
                      </button>
                    </>
                  )}
                </div>

                {/* 色板直接铺在行内，不用浮层，避免被列表的滚动区域裁掉 */}
                {editing && (
                  <ColorSwatches value={editColor} onChange={setEditColor} />
                )}

                {confirming && (
                  <div className="cm-fade-in mt-1.5 rounded-md p-1.5 ">
                    <p className="text-[10px] leading-relaxed text-red-600 dark:text-[var(--cm-danger)]">
                      删除后 {group.item_count} 条记录会变成「未分组」，记录本身不会被删除。
                    </p>
                    <div className="mt-1 flex justify-end gap-1.5">
                      <button
                        type="button"
                        onClick={() => setConfirmDeleteId(null)}
                        className="rounded px-1.5 py-0.5 text-[10px] text-[var(--cm-fg-muted)] hover:bg-black/[0.05] dark:hover:bg-white/[0.08]"
                      >
                        取消
                      </button>
                      <button
                        type="button"
                        onClick={() => handleDelete(group.id)}
                        disabled={busy}
                        className="rounded bg-[var(--cm-danger)] px-1.5 py-0.5 text-[10px] font-medium text-white hover:brightness-110 disabled:opacity-40"
                      >
                        确认删除
                      </button>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
          {groups.length === 0 && (
            <p className="py-3 text-center text-xs text-[var(--cm-fg-faint)]">
              还没有分组，在下面新建一个
            </p>
          )}
        </div>

        {/* 新建 */}
        <div className="mt-3 rounded-lg border border-dashed border-[var(--cm-line)] p-2 dark:border-white/10">
          <div className="flex items-center gap-2">
            <span
              className="h-3 w-3 shrink-0 rounded-full"
              style={{ backgroundColor: newColor }}
            />
            <input
              type="text"
              value={newName}
              onChange={(event) => setNewName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") handleCreate();
              }}
              placeholder="新分组名称，例如：工作"
              className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-[var(--cm-fg-faint)]"
            />
            <button
              type="button"
              onClick={handleCreate}
              disabled={busy || !newName.trim()}
              className="rounded-md bg-[var(--cm-accent)] px-2.5 py-1 text-[10px] font-medium text-white transition-colors hover:bg-[var(--cm-accent-text)] disabled:opacity-40"
            >
              添加
            </button>
          </div>
          <ColorSwatches value={newColor} onChange={setNewColor} />
        </div>

        {error && <p className="mt-2 text-xs text-[var(--cm-danger)]">⚠ {error}</p>}

        <div className="modal-footer">
          <button
            type="button"
            onClick={onClose}
            className="button button--primary"
          >
            完成
          </button>
        </div>
      </div>
    </div>
  );
}

function ColorSwatches({
  value,
  onChange,
}: {
  value: string;
  onChange: (color: string) => void;
}) {
  return (
    <div className="mt-1.5 flex flex-wrap gap-1 pl-5">
      {GROUP_COLORS.map((color) => (
        <button
          key={color}
          type="button"
          aria-label={`选择颜色 ${color}`}
          aria-pressed={color === value}
          onClick={() => onChange(color)}
          className={`h-4 w-4 rounded-full transition-transform hover:scale-110 ${
            color === value ? "ring-2 ring-[var(--cm-accent)] ring-offset-1" : ""
          }`}
          style={{ backgroundColor: color }}
        />
      ))}
    </div>
  );
}
