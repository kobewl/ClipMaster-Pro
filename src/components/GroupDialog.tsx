import { useState } from "react";
import type { ClipGroup } from "@/types/clipboard";
import { GROUP_COLORS } from "@/types/clipboard";
import { commands } from "@/lib/commands";

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

  async function handleCreate() {
    if (!newName.trim()) return;
    await commands.createGroup({ name: newName.trim(), color: newColor });
    setNewName("");
    onChanged();
  }

  async function handleUpdate(id: string) {
    if (!editName.trim()) return;
    await commands.updateGroup({ id, name: editName.trim(), color: editColor });
    setEditId(null);
    onChanged();
  }

  async function handleDelete(id: string) {
    await commands.deleteGroup(id);
    onChanged();
  }

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 backdrop-blur-[2px]"
      onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="w-[320px] rounded-xl bg-white p-4 shadow-2xl dark:bg-neutral-800">
        <h2 className="text-sm font-semibold text-neutral-800 dark:text-neutral-100">
          管理分组
        </h2>

        {/* Existing groups */}
        <div className="mt-3 max-h-[200px] space-y-1.5 overflow-y-auto">
          {groups.map((g) => (
            <div key={g.id} className="flex items-center gap-2 rounded-lg bg-black/[0.02] px-2 py-1.5 dark:bg-white/[0.04]">
              {editId === g.id ? (
                <>
                  <ColorPicker value={editColor} onChange={setEditColor} />
                  <input
                    type="text"
                    value={editName}
                    onChange={(e) => setEditName(e.target.value)}
                    onKeyDown={(e) => { if (e.key === "Enter") handleUpdate(g.id); }}
                    className="min-w-0 flex-1 rounded border border-black/10 bg-transparent px-1.5 py-0.5 text-xs outline-none focus:border-blue-500/40 dark:border-white/10"
                    autoFocus
                  />
                  <button type="button" onClick={() => handleUpdate(g.id)} className="text-[10px] text-blue-500 hover:text-blue-600">保存</button>
                  <button type="button" onClick={() => setEditId(null)} className="text-[10px] text-neutral-400 hover:text-neutral-600">取消</button>
                </>
              ) : (
                <>
                  <span className="h-3 w-3 shrink-0 rounded-full" style={{ backgroundColor: g.color }} />
                  <span className="min-w-0 flex-1 truncate text-xs text-neutral-700 dark:text-neutral-200">{g.name}</span>
                  <span className="text-[10px] text-neutral-400">{g.item_count}</span>
                  <button
                    type="button"
                    onClick={() => { setEditId(g.id); setEditName(g.name); setEditColor(g.color); }}
                    className="text-[10px] text-neutral-400 hover:text-neutral-600"
                  >
                    编辑
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDelete(g.id)}
                    className="text-[10px] text-red-400 hover:text-red-600"
                  >
                    删除
                  </button>
                </>
              )}
            </div>
          ))}
          {groups.length === 0 && (
            <p className="py-3 text-center text-xs text-neutral-400">还没有分组</p>
          )}
        </div>

        {/* Create new */}
        <div className="mt-3 flex items-center gap-2 rounded-lg border border-dashed border-black/10 p-2 dark:border-white/10">
          <ColorPicker value={newColor} onChange={setNewColor} />
          <input
            type="text"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); }}
            placeholder="新分组名称"
            className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-neutral-400"
          />
          <button
            type="button"
            onClick={handleCreate}
            disabled={!newName.trim()}
            className="rounded-md bg-blue-500 px-2.5 py-1 text-[10px] font-medium text-white transition-colors hover:bg-blue-600 disabled:opacity-40"
          >
            添加
          </button>
        </div>

        <div className="mt-3 flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg px-3 py-1.5 text-xs text-neutral-500 transition-colors hover:bg-black/[0.05] dark:hover:bg-white/[0.08]"
          >
            完成
          </button>
        </div>
      </div>
    </div>
  );
}

function ColorPicker({ value, onChange }: { value: string; onChange: (c: string) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="h-4 w-4 shrink-0 rounded-full ring-1 ring-black/10 transition-shadow hover:ring-2 dark:ring-white/20"
        style={{ backgroundColor: value }}
      />
      {open && (
        <div className="absolute left-0 top-6 z-50 flex gap-1 rounded-lg bg-white p-1.5 shadow-xl ring-1 ring-black/10 dark:bg-neutral-700 dark:ring-white/10">
          {GROUP_COLORS.map((c) => (
            <button
              key={c}
              type="button"
              onClick={() => { onChange(c); setOpen(false); }}
              className={`h-5 w-5 rounded-full transition-transform hover:scale-110 ${c === value ? "ring-2 ring-blue-500 ring-offset-1" : ""}`}
              style={{ backgroundColor: c }}
            />
          ))}
        </div>
      )}
    </div>
  );
}
