import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AppSettings, UpdateStatus } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import { onUpdateInstalling, onUpdateProgress } from "@/lib/events";
import {
  applyTheme,
  isThemePreference,
  THEME_OPTIONS,
  type ThemePreference,
} from "@/lib/theme";
import { ShortcutInput } from "./ShortcutInput";
import { ExcludedAppsSection } from "./ExcludedAppsSection";
import { Icon } from "./Icon";

// 更新说明用 react-markdown + remark-gfm 渲染，只有检查到新版本时才用得上，
// 单独拆一个 chunk，打开设置面板本身不必加载 markdown 栈。
const ReleaseNotes = lazy(() =>
  import("./ReleaseNotes").then((m) => ({ default: m.ReleaseNotes })),
);

/**
 * 设置里展示的更新说明：去掉首尾空白，但**保留全部行**。
 *
 * 旧实现只取第一行非空行，GitHub release 的标题之后整段正文都会被丢掉。
 * 空字符串或只含空白时返回 null，调用方据此决定要不要画这块。
 */
export function visibleReleaseNotes(notes: string | null | undefined): string | null {
  if (!notes) return null;
  const text = notes.replace(/\r\n/g, "\n").trim();
  return text.length > 0 ? text : null;
}

/** 发现新版本时跳去这里下载（更新通道就绪前，「检查更新」也会提示这条路径）。 */
const RELEASES_URL = "https://github.com/kobewl/ClipMaster-Pro/releases/latest";

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
  const [theme, setTheme] = useState<ThemePreference>("system");
  const [themeSaving, setThemeSaving] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [shortcutSaving, setShortcutSaving] = useState(false);

  // 开机自启：状态来自系统而不是 settings，且点一下立即生效，不走「保存」。
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartSaving, setAutostartSaving] = useState(false);
  const [autostartError, setAutostartError] = useState<string | null>(null);

  // 应用版本 + 检查更新。
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  /** 应用内更新流程：downloading 时展示进度条，installing 提示即将重启。 */
  const [updateDownloading, setUpdateDownloading] = useState(false);
  const [updateProgress, setUpdateProgress] = useState(0);
  const [updateInstalling, setUpdateInstalling] = useState(false);

  // 快捷键保存失败时要回滚成"当前真实生效的值"，用 ref 保证拿到的是最新的 props。
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  // 上一次同步进草稿的服务端设置。只同步真正变了的字段：立即生效的通道
  // （排除名单、主题、快捷键）每次都会产生新的 settings 对象，整份覆盖会把
  // 别的未保存草稿（最大历史数量、保留天数、采集开关）冲掉；而托盘切换暂停
  // 改的 capture_enabled 仍要同步进来，否则再点保存会把托盘的设置覆盖回去。
  const syncedSettingsRef = useRef<AppSettings | null>(null);

  useEffect(() => {
    if (!settings) return;
    const prev = syncedSettingsRef.current;
    syncedSettingsRef.current = settings;
    if (!prev || prev.max_history !== settings.max_history) {
      setMaxHistory(String(settings.max_history));
    }
    if (!prev || prev.retention_days !== settings.retention_days) {
      setRetentionDays(String(settings.retention_days));
    }
    if (!prev || prev.capture_enabled !== settings.capture_enabled) {
      setCaptureEnabled(settings.capture_enabled);
    }
    if (!prev || prev.shortcut !== settings.shortcut) {
      setShortcut(settings.shortcut);
    }
    if (!prev || prev.theme !== settings.theme) {
      setTheme(isThemePreference(settings.theme) ? settings.theme : "system");
    }
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
    // 版本号不常变，读到一次就够；失败也不影响面板其它部分。
    getVersion().then(setAppVersion).catch(() => {});
  }, [open]);

  // 更新进度事件：面板开着才 meaningful，但常驻监听也无妨（没有更新流程时事件不会来）。
  useEffect(() => {
    const unlistenProgress = onUpdateProgress((percent) => setUpdateProgress(percent));
    const unlistenInstalling = onUpdateInstalling(() => {
      setUpdateDownloading(false);
      setUpdateInstalling(true);
    });
    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenInstalling.then((fn) => fn());
    };
  }, []);

  if (!open) return null;

  async function handleSave() {
    const current = settingsRef.current;
    // 排除名单是整体保存的一部分：设置还没读到就保存，会把名单写成空。
    if (!current) {
      setError("设置尚未加载完成，请稍后再试");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await onSave({
        max_history: parseIntInRange(
          maxHistory,
          current.max_history,
          MAX_HISTORY_RANGE,
        ),
        retention_days: parseIntInRange(
          retentionDays,
          current.retention_days,
          RETENTION_DAYS_RANGE,
        ),
        capture_enabled: captureEnabled,
        shortcut,
        theme,
        excluded_apps: current.excluded_apps,
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

  /** 主题立即生效并落库，不必等点「保存」。 */
  async function handleThemeChange(next: ThemePreference) {
    const current = settingsRef.current;
    if (!current || next === theme) return;
    const previous = theme;
    setTheme(next);
    applyTheme(next);
    setThemeSaving(true);
    setError(null);
    try {
      await onSave({
        max_history: current.max_history,
        retention_days: current.retention_days,
        capture_enabled: current.capture_enabled,
        shortcut: current.shortcut,
        theme: next,
        excluded_apps: current.excluded_apps,
      });
      onSettingsChange({ ...current, theme: next });
    } catch (err: unknown) {
      setTheme(previous);
      applyTheme(previous);
      setError(isCommandError(err) ? err.message : "主题设置失败");
    } finally {
      setThemeSaving(false);
    }
  }

  /** 应用更新入口见 docs/release/RELEASE.md；更新源没配置时后端返回稳定的错误码。 */
  async function handleCheckUpdates() {
    setUpdateChecking(true);
    setUpdateError(null);
    setUpdateStatus(null);
    try {
      const status = await commands.checkForUpdates();
      setUpdateStatus(status);
    } catch (err: unknown) {
      setUpdateError(isCommandError(err) ? err.message : "检查更新失败");
    } finally {
      setUpdateChecking(false);
    }
  }

  /** 应用内一键更新：下载 → 验签安装 → 自动重启。进度靠 update-progress 事件。 */
  async function handleInstallUpdate() {
    setUpdateError(null);
    setUpdateDownloading(true);
    setUpdateProgress(0);
    try {
      await commands.downloadAndInstallUpdate();
      // 正常情况不会走到这里：安装完应用直接重启了。
    } catch (err: unknown) {
      // 失败要退出「下载中」状态，让按钮恢复可点。
      setUpdateDownloading(false);
      setUpdateInstalling(false);
      setUpdateProgress(0);
      setUpdateError(isCommandError(err) ? err.message : "安装更新失败");
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

  const releaseNotes = visibleReleaseNotes(updateStatus?.notes);

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
          <legend className="px-1.5 text-[11px] font-medium text-[var(--cm-fg-faint)]">数据保留</legend>
          <label className="block text-xs text-[var(--cm-fg-muted)]">
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
              className="mt-1 w-full rounded-lg border border-[var(--cm-line)] bg-transparent px-2.5 py-1.5 text-sm text-neutral-800 outline-none focus:border-[var(--cm-accent-ring)] focus:ring-1 focus:ring-[var(--cm-accent-ring)] dark:text-neutral-100"
            />
            <span className="mt-1 block text-[10px] text-[var(--cm-fg-faint)]">
              超出上限时自动清理最旧的记录
            </span>
          </label>
          <label className="mt-3 block text-xs text-[var(--cm-fg-muted)]">
            保留天数 <span className="text-[10px] text-[var(--cm-fg-faint)]">（0 = 不按天清理）</span>
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
              className="mt-1 w-full rounded-lg border border-[var(--cm-line)] bg-transparent px-2.5 py-1.5 text-sm text-neutral-800 outline-none focus:border-[var(--cm-accent-ring)] focus:ring-1 focus:ring-[var(--cm-accent-ring)] dark:text-neutral-100"
            />
          </label>
          <label className="mt-3 flex cursor-pointer items-center gap-2 text-xs text-[var(--cm-fg-muted)]">
            <input
              type="checkbox"
              checked={captureEnabled}
              onChange={(event) => setCaptureEnabled(event.target.checked)}
              className="accent-[var(--cm-accent)]"
            />
            启用剪贴板采集
          </label>
        </fieldset>

        {settings && (
          <ExcludedAppsSection
            excludedApps={settings.excluded_apps}
            onSettingsChange={onSettingsChange}
          />
        )}

        <fieldset className="settings-section mt-3">
          <legend className="px-1.5 text-[11px] font-medium text-[var(--cm-fg-faint)]">全局快捷键</legend>
          <label className="block text-xs text-[var(--cm-fg-muted)]">
            显示/隐藏主窗口
            <ShortcutInput
              value={shortcut}
              onChange={handleShortcutChange}
              disabled={shortcutSaving}
              error={shortcutError}
            />
          </label>
          <p className="mt-2 text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">
            点击后按新组合即可修改 · 改完立即生效 · Delete 清除 · Esc 取消
          </p>
        </fieldset>

        <fieldset className="settings-section mt-3">
          <legend className="px-1.5 text-[11px] font-medium text-[var(--cm-fg-faint)]">外观</legend>
          <p className="mb-2 text-xs text-[var(--cm-fg-muted)]">主题</p>
          <div className="flex flex-wrap gap-1">
            {THEME_OPTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                disabled={themeSaving}
                aria-pressed={theme === option.value}
                onClick={() => handleThemeChange(option.value)}
                className={
                  theme === option.value
                    ? "button button--primary button--compact"
                    : "button button--secondary button--compact"
                }
              >
                {option.label}
              </button>
            ))}
          </div>
          <p className="mt-2 text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">
            「跟随系统」会随 macOS 浅色/深色自动切换；改完立即生效。
          </p>
        </fieldset>

        <fieldset className="settings-section mt-3">
          <legend className="px-1.5 text-[11px] font-medium text-[var(--cm-fg-faint)]">系统</legend>
          <label className="flex cursor-pointer items-center gap-2 text-xs text-[var(--cm-fg-muted)]">
            <input
              type="checkbox"
              checked={autostart ?? false}
              disabled={autostart === null || autostartSaving}
              onChange={(event) => handleAutostartToggle(event.target.checked)}
              className="accent-[var(--cm-accent)] disabled:opacity-50"
            />
            {autostartSaving ? "设置中…" : "开机时自动启动"}
          </label>
          <p className="mt-2 text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">
            {autostart === null && !autostartError
              ? "正在读取…"
              : "启动后安静地待在后台，不会弹出窗口；按全局快捷键随时唤起。"}
          </p>
          {autostartError && (
            <p className="mt-1.5 text-[10px] leading-relaxed text-[var(--cm-danger)]">⚠ {autostartError}</p>
          )}

          <div className="mt-3 flex items-center justify-between gap-2 border-t border-black/[0.06] pt-3 dark:border-white/[0.08]">
            <div className="text-xs text-[var(--cm-fg-muted)]">
              应用版本{" "}
              <span className="tabular-nums text-[var(--cm-fg-faint)]">{appVersion ?? "…"}</span>
            </div>
            <button
              type="button"
              onClick={handleCheckUpdates}
              disabled={updateChecking}
              className="button button--secondary button--compact"
            >
              {updateChecking ? "检查中…" : "检查更新"}
            </button>
          </div>
          {updateStatus && !updateStatus.update_available && (
            <p className="mt-1.5 text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">
              已是最新版本 ✓
            </p>
          )}
          {updateStatus?.update_available && (
            <div className="mt-1.5 space-y-1.5">
              <div className="flex items-center justify-between gap-2">
                <p className="text-[10px] leading-relaxed text-[var(--cm-success)]">
                  发现新版本 {updateStatus.latest_version}（当前 {updateStatus.current_version}）
                </p>
                {updateInstalling ? (
                  <p className="text-[10px] text-[var(--cm-fg-muted)]">正在安装，即将重启…</p>
                ) : updateDownloading ? (
                  <p className="tabular-nums text-[10px] text-[var(--cm-fg-muted)]">
                    下载中 {updateProgress}%
                  </p>
                ) : (
                  <div className="flex items-center gap-1.5">
                    <button
                      type="button"
                      onClick={handleInstallUpdate}
                      className="button button--primary button--compact"
                    >
                      立即更新
                    </button>
                    <button
                      type="button"
                      onClick={() => openUrl(updateStatus.release_url ?? RELEASES_URL)}
                      className="button button--secondary button--compact"
                    >
                      前往下载
                    </button>
                  </div>
                )}
              </div>
              {updateDownloading && (
                <div
                  className="h-1 w-full overflow-hidden rounded-full bg-[var(--cm-surface3)]"
                  role="progressbar"
                  aria-label="更新下载进度"
                >
                  <div
                    className="h-full rounded-full bg-[var(--cm-accent)] transition-[width] duration-200"
                    style={{ width: `${Math.max(updateProgress, 3)}%` }}
                  />
                </div>
              )}
              {releaseNotes && (
                <Suspense fallback={null}>
                  <ReleaseNotes markdown={releaseNotes} />
                </Suspense>
              )}
            </div>
          )}
          {updateError && (
            <p className="mt-1.5 text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">⚠ {updateError}</p>
          )}
        </fieldset>

        {error && <p className="mt-2 text-xs text-[var(--cm-danger)]">⚠ {error}</p>}

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
