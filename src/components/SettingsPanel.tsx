import { useEffect, useState } from "react";
import type { AppSettings } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import { ShortcutInput } from "./ShortcutInput";

interface Props {
  open: boolean;
  settings: AppSettings | null;
  onClose: () => void;
  onSave: (settings: AppSettings) => Promise<void>;
  onSettingsChange: (settings: AppSettings) => void;
}

export function SettingsPanel({ open, settings, onClose, onSave, onSettingsChange }: Props) {
  const [maxHistory, setMaxHistory] = useState(1000);
  const [retentionDays, setRetentionDays] = useState(30);
  const [captureEnabled, setCaptureEnabled] = useState(true);
  const [shortcut, setShortcut] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [shortcutSaving, setShortcutSaving] = useState(false);

  useEffect(() => {
    if (settings) {
      setMaxHistory(settings.max_history);
      setRetentionDays(settings.retention_days);
      setCaptureEnabled(settings.capture_enabled);
      setShortcut(settings.shortcut);
    }
  }, [settings]);

  if (!open) return null;

  async function handleSave() {
    setSaving(true);
    setError(null);
    try {
      await onSave({ max_history: maxHistory, retention_days: retentionDays, capture_enabled: captureEnabled, shortcut });
      onClose();
    } catch {
      setError("保存失败");
    } finally {
      setSaving(false);
    }
  }

  async function handleShortcutChange(newShortcut: string) {
    setShortcut(newShortcut);
    setShortcutError(null);
    setShortcutSaving(true);
    try {
      const updated = await commands.updateShortcut(newShortcut);
      setShortcut(updated.shortcut);
      onSettingsChange(updated);
    } catch (err: unknown) {
      if (isCommandError(err) && err.code === "shortcut_conflict") {
        setShortcutError(err.message);
      } else {
        setShortcutError("快捷键设置失败");
      }
      if (settings) setShortcut(settings.shortcut);
    } finally {
      setShortcutSaving(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 backdrop-blur-[2px]"
      onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="w-[340px] rounded-xl bg-white p-5 shadow-2xl dark:bg-neutral-800">
        <h2 className="text-sm font-semibold text-neutral-800 dark:text-neutral-100">⚙ 设置</h2>

        <fieldset className="mt-4 rounded-lg border border-black/[0.06] p-3 dark:border-white/[0.08]">
          <legend className="px-1.5 text-[11px] font-medium text-neutral-400">数据保留</legend>
          <label className="block text-xs text-neutral-500 dark:text-neutral-400">
            最大历史数量
            <input type="number" min={10} max={100000} value={maxHistory}
              onChange={(e) => setMaxHistory(Number(e.target.value))}
              className="mt-1 w-full rounded-lg border border-black/[0.08] bg-transparent px-2.5 py-1.5 text-sm text-neutral-800 outline-none focus:border-blue-500/40 focus:ring-1 focus:ring-blue-500/20 dark:border-white/[0.1] dark:text-neutral-100" />
          </label>
          <label className="mt-3 block text-xs text-neutral-500 dark:text-neutral-400">
            保留天数 <span className="text-[10px] text-neutral-400">（0=不按天清理）</span>
            <input type="number" min={0} max={3650} value={retentionDays}
              onChange={(e) => setRetentionDays(Number(e.target.value))}
              className="mt-1 w-full rounded-lg border border-black/[0.08] bg-transparent px-2.5 py-1.5 text-sm text-neutral-800 outline-none focus:border-blue-500/40 focus:ring-1 focus:ring-blue-500/20 dark:border-white/[0.1] dark:text-neutral-100" />
          </label>
          <label className="mt-3 flex cursor-pointer items-center gap-2 text-xs text-neutral-500 dark:text-neutral-400">
            <input type="checkbox" checked={captureEnabled}
              onChange={(e) => setCaptureEnabled(e.target.checked)}
              className="accent-blue-500" />
            启用剪贴板采集
          </label>
        </fieldset>

        <fieldset className="mt-3 rounded-lg border border-black/[0.06] p-3 dark:border-white/[0.08]">
          <legend className="px-1.5 text-[11px] font-medium text-neutral-400">全局快捷键</legend>
          <label className="block text-xs text-neutral-500 dark:text-neutral-400">
            显示/隐藏主窗口
            <ShortcutInput value={shortcut} onChange={handleShortcutChange}
              disabled={shortcutSaving} error={shortcutError} />
          </label>
          <p className="mt-2 text-[10px] leading-relaxed text-neutral-400">
            点击后按新组合即可修改 · Delete 清除 · Esc 取消
          </p>
        </fieldset>

        {error && <p className="mt-2 text-xs text-red-500">⚠ {error}</p>}

        <div className="mt-4 flex justify-end gap-2">
          <button type="button" onClick={onClose}
            className="rounded-lg px-3 py-1.5 text-xs text-neutral-500 hover:bg-black/[0.05] dark:hover:bg-white/[0.08]">
            取消
          </button>
          <button type="button" onClick={handleSave} disabled={saving}
            className="rounded-lg bg-blue-500 px-4 py-1.5 text-xs font-medium text-white hover:bg-blue-600 disabled:opacity-50">
            {saving ? "保存中…" : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}
