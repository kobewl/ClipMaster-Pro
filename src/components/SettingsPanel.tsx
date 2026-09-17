import { useEffect, useRef, useState } from "react";
import type { AppSettings } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import { ShortcutInput } from "./ShortcutInput";
import { Icon } from "./Icon";

interface Props {
  open: boolean;
  settings: AppSettings | null;
  onClose: () => void;
  onSave: (settings: AppSettings) => Promise<void>;
  onSettingsChange: (settings: AppSettings) => void;
}

const MAX_HISTORY_RANGE = { min: 10, max: 100_000 } as const;
const RETENTION_DAYS_RANGE = { min: 0, max: 3650 } as const;

/**
 * 把输入框里的字符串解析成整数。
 * 输入框允许被清空，所以不能直接 Number("")（那是 0，会把"最大历史"悄悄改没），
 * 解析不出数字时回退到原值。
 */
function parseIntInRange(
  raw: string,
  fallback: number,
  range: { min: number; max: number },
): number {
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed)) return fallback;
  return Math.min(range.max, Math.max(range.min, parsed));
}

export function SettingsPanel({
  open,
  settings,
  onClose,
  onSave,
  onSettingsChange,
}: Props) {
  // 数字字段用字符串存，这样用户可以把内容删光重新输入。
  const [maxHistory, setMaxHistory] = useState("1000");
  const [retentionDays, setRetentionDays] = useState("30");
  const [captureEnabled, setCaptureEnabled] = useState(true);
  const [shortcut, setShortcut] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [shortcutSaving, setShortcutSaving] = useState(false);

  // 开机自启：状态来自系统而不是 settings，且点一下立即生效，不走「保存」。
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartSaving, setAutostartSaving] = useState(false);
  const [autostartError, setAutostartError] = useState<string | null>(null);

  // 快捷键保存失败时要回滚成"当前真实生效的值"，用 ref 保证拿到的是最新的 props。
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  useEffect(() => {
    if (!settings) return;
    setMaxHistory(String(settings.max_history));
    setRetentionDays(String(settings.retention_days));
    setCaptureEnabled(settings.capture_enabled);
    setShortcut(settings.shortcut);
  }, [settings]);

  // 每次打开面板都清掉上一次的错误提示，并重新读一次自启状态
  // （用户可能在系统设置里改过，缓存一份会显示成错的）。
  useEffect(() => {
    if (!open) return;
    setError(null);
    setAutostartError(null);
    setAutostart(null);
    commands
      .getAutostartEnabled()
      .then(setAutostart)
      .catch(() => setAutostartError("读取开机自启状态失败"));
  }, [open]);

  if (!open) return null;

  async function handleSave() {
    const current = settingsRef.current;
    setSaving(true);
    setError(null);
    try {
      await onSave({
        max_history: parseIntInRange(
          maxHistory,
          current?.max_history ?? 1000,
          MAX_HISTORY_RANGE,
        ),
        retention_days: parseIntInRange(
          retentionDays,
          current?.retention_days ?? 30,
          RETENTION_DAYS_RANGE,
        ),
        capture_enabled: captureEnabled,
        shortcut,
      });
      onClose();
    } catch (err: unknown) {
      setError(isCommandError(err) ? err.message : "保存失败");
    } finally {
      setSaving(false);
    }
  }

  /** 快捷键单独走一条立即生效的通道：后端需要重新向系统注册热键，不能等点"保存"。 */
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
        setShortcutError(isCommandError(err) ? err.message : "快捷键设置失败");
      }
      setShortcut(settingsRef.current?.shortcut ?? "");
    } finally {
      setShortcutSaving(false);
    }
  }

  /**
   * 开机自启同样立即生效：它改的是系统里的登录项，不是本应用的配置。
   *
   * 失败时把开关**弹回原值** —— 让控件停在用户点的位置上，他会以为已经设好了。
   */
  async function handleAutostartToggle(next: boolean) {
    const previous = autostart;
    setAutostartError(null);
    setAutostart(next); // 先动一下，点击有即时反馈
    setAutostartSaving(true);
    try {
      await commands.setAutostartEnabled(next);
    } catch (err: unknown) {
      setAutostart(previous);
      setAutostartError(isCommandError(err) ? err.message : "设置开机自启失败");
    } finally {
      setAutostartSaving(false);
    }
  }

  return (
    <div
      className="modal-backdrop cm-fade-in"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal-card settings-card cm-pop-in">
        <div className="modal-header">
          <span className="modal-header__icon"><Icon name="settings" /></span>
          <div><h2>偏好设置</h2><p>让 ClipMaster 更符合你的工作方式</p></div>
          <button className="icon-button" onClick={onClose} aria-label="关闭设置"><Icon name="close" /></button>
        </div>

        <fieldset className="settings-section mt-4">
          <legend className="px-1.5 text-[11px] font-medium text-neutral-400">数据保留</legend>
          <label className="block text-xs text-neutral-500 dark:text-neutral-400">
            最大历史数量
            <input
              type="number"
              inputMode="numeric"
              min={MAX_HISTORY_RANGE.min}
              max={MAX_HISTORY_RANGE.max}
              value={maxHistory}
              onChange={(event) => setMaxHistory(event.target.value)}
              onBlur={() =>
                setMaxHistory(
                  String(
                    parseIntInRange(
                      maxHistory,
                      settingsRef.current?.max_history ?? 1000,
                      MAX_HISTORY_RANGE,
                    ),
                  ),
                )
              }
              className="mt-1 w-full rounded-lg border border-black/[0.08] bg-transparent px-2.5 py-1.5 text-sm text-neutral-800 outline-none focus:border-blue-500/40 focus:ring-1 focus:ring-blue-500/20 dark:border-white/[0.1] dark:text-neutral-100"
            />
            <span className="mt-1 block text-[10px] text-neutral-400">
              超出上限时自动清理最旧的记录
            </span>
          </label>
          <label className="mt-3 block text-xs text-neutral-500 dark:text-neutral-400">
            保留天数 <span className="text-[10px] text-neutral-400">（0 = 不按天清理）</span>
            <input
              type="number"
              inputMode="numeric"
              min={RETENTION_DAYS_RANGE.min}
              max={RETENTION_DAYS_RANGE.max}
              value={retentionDays}
              onChange={(event) => setRetentionDays(event.target.value)}
              onBlur={() =>
                setRetentionDays(
                  String(
                    parseIntInRange(
                      retentionDays,
                      settingsRef.current?.retention_days ?? 30,
                      RETENTION_DAYS_RANGE,
                    ),
                  ),
                )
              }
              className="mt-1 w-full rounded-lg border border-black/[0.08] bg-transparent px-2.5 py-1.5 text-sm text-neutral-800 outline-none focus:border-blue-500/40 focus:ring-1 focus:ring-blue-500/20 dark:border-white/[0.1] dark:text-neutral-100"
            />
          </label>
          <label className="mt-3 flex cursor-pointer items-center gap-2 text-xs text-neutral-500 dark:text-neutral-400">
            <input
              type="checkbox"
              checked={captureEnabled}
              onChange={(event) => setCaptureEnabled(event.target.checked)}
              className="accent-blue-500"
            />
            启用剪贴板采集
          </label>
        </fieldset>

        <fieldset className="settings-section mt-3">
          <legend className="px-1.5 text-[11px] font-medium text-neutral-400">全局快捷键</legend>
          <label className="block text-xs text-neutral-500 dark:text-neutral-400">
            显示/隐藏主窗口
            <ShortcutInput
              value={shortcut}
              onChange={handleShortcutChange}
              disabled={shortcutSaving}
              error={shortcutError}
            />
          </label>
          <p className="mt-2 text-[10px] leading-relaxed text-neutral-400">
            点击后按新组合即可修改 · 改完立即生效 · Delete 清除 · Esc 取消
          </p>
        </fieldset>

        <fieldset className="settings-section mt-3">
          <legend className="px-1.5 text-[11px] font-medium text-neutral-400">系统</legend>
          <label className="flex cursor-pointer items-center gap-2 text-xs text-neutral-500 dark:text-neutral-400">
            <input
              type="checkbox"
              checked={autostart ?? false}
              disabled={autostart === null || autostartSaving}
              onChange={(event) => handleAutostartToggle(event.target.checked)}
              className="accent-blue-500 disabled:opacity-50"
            />
            {autostartSaving ? "设置中…" : "开机时自动启动"}
          </label>
          <p className="mt-2 text-[10px] leading-relaxed text-neutral-400">
            {autostart === null && !autostartError
              ? "正在读取…"
              : "启动后安静地待在后台，不会弹出窗口；按全局快捷键随时唤起。"}
          </p>
          {autostartError && (
            <p className="mt-1.5 text-[10px] leading-relaxed text-red-500">⚠ {autostartError}</p>
          )}
        </fieldset>

        {error && <p className="mt-2 text-xs text-red-500">⚠ {error}</p>}

        <div className="modal-footer">
          <button
            type="button"
            onClick={onClose}
            className="button button--secondary"
          >
            取消
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={saving}
            className="button button--primary"
          >
            {saving ? "保存中…" : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}
