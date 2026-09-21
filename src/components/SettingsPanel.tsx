import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AgentConfigInfo, AppSettings, UpdateStatus } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";
import {
  AGENT_PRESETS,
  normalizeAgentKey,
  validateAgentBaseUrl,
  validateAgentKey,
} from "@/lib/agentKey";
import { onUpdateInstalling, onUpdateProgress } from "@/lib/events";
import {
  applyTheme,
  isThemePreference,
  THEME_OPTIONS,
  type ThemePreference,
} from "@/lib/theme";
import { ShortcutInput } from "./ShortcutInput";
import { Icon } from "./Icon";

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

  // AI 助手：配置快照只在打开面板时读一次；密钥是单向写入，读不回来。
  const [agentConfig, setAgentConfig] = useState<AgentConfigInfo | null>(null);
  const [agentKeyInput, setAgentKeyInput] = useState("");
  const [agentBaseUrl, setAgentBaseUrl] = useState("");
  const [agentModel, setAgentModel] = useState("");
  /** 端点区是否展开。默认收起，避免把设置面板撑得太长。 */
  const [agentEndpointOpen, setAgentEndpointOpen] = useState(false);
  const [agentSaving, setAgentSaving] = useState(false);
  const [agentEndpointSaving, setAgentEndpointSaving] = useState(false);
  const [agentTesting, setAgentTesting] = useState(false);
  const [agentRunsClearing, setAgentRunsClearing] = useState(false);
  /** 上一次清除删掉的条数（用于给出「真的删了」的反馈）。 */
  const [agentRunsCleared, setAgentRunsCleared] = useState<number | null>(null);
  const [agentError, setAgentError] = useState<string | null>(null);
  const [agentEndpointError, setAgentEndpointError] = useState<string | null>(null);
  const [agentNotice, setAgentNotice] = useState<string | null>(null);

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

  useEffect(() => {
    if (!settings) return;
    setMaxHistory(String(settings.max_history));
    setRetentionDays(String(settings.retention_days));
    setCaptureEnabled(settings.capture_enabled);
    setShortcut(settings.shortcut);
    setTheme(isThemePreference(settings.theme) ? settings.theme : "system");
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
    // AI 配置每次打开都重读：密钥可能在上一轮里被存进/清出钥匙串，
    // 而且钥匙串被锁时会在这里就报出来，不必等用户点了 AI 按钮才知道。
    setAgentConfig(null);
    setAgentKeyInput("");
    setAgentError(null);
    setAgentEndpointError(null);
    setAgentNotice(null);
    setAgentEndpointOpen(false);
    setAgentRunsCleared(null);
    commands
      .getAgentConfig()
      .then((info) => {
        setAgentConfig(info);
        setAgentBaseUrl(info.base_url);
        setAgentModel(info.model);
      })
      .catch((err: unknown) =>
        setAgentError(isCommandError(err) ? err.message : "读取 AI 配置失败"),
      );
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
        theme,
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
   * 保存 DeepSeek API Key。和快捷键一样走"立即生效"通道：它写的是系统钥匙串
   * 而不是本应用的设置表，等用户再点一次"保存"没有意义。
   */
  async function handleAgentKeySave() {
    const key = normalizeAgentKey(agentKeyInput);
    const invalid = validateAgentKey(key);
    setAgentError(null);
    setAgentNotice(null);
    if (invalid) {
      setAgentError(invalid);
      return;
    }
    setAgentSaving(true);
    try {
      const info = await commands.saveAgentKey(key);
      setAgentConfig(info);
      setAgentKeyInput("");
      setAgentNotice("已保存到系统钥匙串 ✓");
    } catch (err: unknown) {
      setAgentError(isCommandError(err) ? err.message : "保存 API Key 失败");
    } finally {
      setAgentSaving(false);
    }
  }

  /** 清除钥匙串里的 Key。开发期环境变量注入的那份不归应用管，会继续生效。 */
  async function handleAgentKeyClear() {
    setAgentError(null);
    setAgentNotice(null);
    setAgentSaving(true);
    try {
      const info = await commands.clearAgentKey();
      setAgentConfig(info);
      setAgentNotice(info.configured ? "已清除钥匙串中的 Key（仍在用环境变量）。" : "已清除 API Key。");
    } catch (err: unknown) {
      setAgentError(isCommandError(err) ? err.message : "清除 API Key 失败");
    } finally {
      setAgentSaving(false);
    }
  }

  /** 保存自定义服务地址与模型名。 */
  async function handleAgentEndpointSave() {
    const invalid = validateAgentBaseUrl(agentBaseUrl);
    setAgentError(null);
    setAgentEndpointError(null);
    setAgentNotice(null);
    if (invalid) {
      setAgentEndpointError(invalid);
      return;
    }
    setAgentEndpointSaving(true);
    try {
      const info = await commands.saveAgentEndpoint(agentBaseUrl.trim(), agentModel.trim());
      setAgentConfig(info);
      setAgentBaseUrl(info.base_url);
      setAgentModel(info.model);
      setAgentNotice("服务地址已更新 ✓");
    } catch (err: unknown) {
      setAgentEndpointError(isCommandError(err) ? err.message : "保存服务地址失败");
    } finally {
      setAgentEndpointSaving(false);
    }
  }

  /** 恢复内置默认地址（改坏了时的退路）。 */
  async function handleAgentEndpointReset() {
    setAgentEndpointError(null);
    setAgentNotice(null);
    setAgentEndpointSaving(true);
    try {
      const info = await commands.resetAgentEndpoint();
      setAgentConfig(info);
      setAgentBaseUrl(info.base_url);
      setAgentModel(info.model);
      setAgentNotice("已恢复默认服务地址 ✓");
    } catch (err: unknown) {
      setAgentEndpointError(isCommandError(err) ? err.message : "恢复默认地址失败");
    } finally {
      setAgentEndpointSaving(false);
    }
  }

  async function handleAgentTest() {
    setAgentError(null);
    setAgentNotice(null);
    setAgentTesting(true);
    try {
      await commands.testAgentConnection();
      setAgentNotice("连接正常 ✓");
    } catch (err: unknown) {
      setAgentError(isCommandError(err) ? err.message : "测试连接失败");
    } finally {
      setAgentTesting(false);
    }
  }

  /**
   * 清除 AI 使用记录。
   *
   * 审计表里只有元数据（哪条记录、哪个服务、花了多久），但「我用 AI 处理过哪些
   * 内容」本身也是隐私，用户该能一键抹掉。剪贴板历史不受影响。
   */
  async function handleAgentRunsClear() {
    setAgentError(null);
    setAgentNotice(null);
    setAgentRunsClearing(true);
    try {
      const removed = await commands.clearAgentRuns();
      setAgentRunsCleared(removed);
      setAgentNotice(
        removed > 0 ? `已清除 ${removed} 条使用记录 ✓` : "没有使用记录需要清除。",
      );
    } catch (err: unknown) {
      setAgentError(isCommandError(err) ? err.message : "清除使用记录失败");
    } finally {
      setAgentRunsClearing(false);
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
          <legend className="px-1.5 text-[11px] font-medium text-[var(--cm-fg-faint)]">AI 助手</legend>
          <div className="flex items-center justify-between gap-2">
            <div className="text-xs text-[var(--cm-fg-muted)]">
              DeepSeek{" "}
              {agentConfig === null ? (
                <span className="text-[var(--cm-fg-faint)]">读取中…</span>
              ) : agentConfig.configured ? (
                <span className="text-[var(--cm-success)]">已配置 ✓</span>
              ) : (
                <span className="text-[var(--cm-fg-faint)]">未配置</span>
              )}
            </div>
            {agentConfig?.configured && (
              <button
                type="button"
                onClick={handleAgentTest}
                disabled={agentTesting || agentSaving}
                className="button button--secondary button--compact"
              >
                {agentTesting ? "测试中…" : "测试连接"}
              </button>
            )}
          </div>

          {agentConfig && (
            <p className="mt-1.5 text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">
              {agentConfig.configured
                ? `当前使用 ${agentConfig.key_hint} · ${
                    agentConfig.key_source === "env" ? "来自开发环境变量" : "已存入系统钥匙串"
                  }`
                : "填入 API Key 后，预览弹窗里的总结 / 翻译 / 解释等动作才会生效。"}
            </p>
          )}

          <div className="mt-2 flex items-center gap-1.5">
            <input
              type="password"
              value={agentKeyInput}
              placeholder={agentConfig?.configured ? "填入新的 Key 以替换" : "sk-…"}
              disabled={agentSaving}
              onChange={(event) => setAgentKeyInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void handleAgentKeySave();
              }}
              className="min-w-0 flex-1 rounded-lg border border-[var(--cm-line)] bg-transparent px-2.5 py-1.5 text-sm text-neutral-800 outline-none focus:border-[var(--cm-accent-ring)] focus:ring-1 focus:ring-[var(--cm-accent-ring)] disabled:opacity-60 dark:text-neutral-100"
            />
            <button
              type="button"
              onClick={handleAgentKeySave}
              disabled={agentSaving || agentKeyInput.trim().length === 0}
              className="button button--primary button--compact"
            >
              {agentSaving ? "保存中…" : "保存"}
            </button>
            {agentConfig?.configured && agentConfig.key_source === "keychain" && (
              <button
                type="button"
                onClick={handleAgentKeyClear}
                disabled={agentSaving}
                className="button button--secondary button--compact"
              >
                清除
              </button>
            )}
          </div>

          {/* 服务地址：默认收起。展开后可以接到任何 OpenAI 兼容服务。 */}
          <button
            type="button"
            onClick={() => setAgentEndpointOpen((open) => !open)}
            aria-expanded={agentEndpointOpen}
            className="mt-2 flex w-full items-center gap-1 text-[10px] text-[var(--cm-fg-muted)]"
          >
            <span>{agentEndpointOpen ? "▾" : "▸"}</span>
            服务地址
            <span className="text-[var(--cm-fg-faint)]">
              {agentConfig?.base_url_is_custom
                ? `（自定义：${agentConfig.base_url}）`
                : "（默认 DeepSeek）"}
            </span>
          </button>

          {agentEndpointOpen && (
            <div className="mt-2 rounded-lg border border-black/[0.06] p-2 dark:border-white/[0.08]">
              <div className="flex flex-wrap gap-1">
                {AGENT_PRESETS.map((preset) => (
                  <button
                    key={preset.label}
                    type="button"
                    onClick={() => {
                      setAgentBaseUrl(preset.baseUrl);
                      setAgentModel(preset.model);
                      setAgentEndpointError(null);
                    }}
                    className="button button--secondary button--compact"
                  >
                    {preset.label}
                  </button>
                ))}
              </div>
              <label className="mt-2 block text-[10px] text-[var(--cm-fg-muted)]">
                API 地址
                <input
                  type="text"
                  value={agentBaseUrl}
                  spellCheck={false}
                  autoComplete="off"
                  placeholder="https://api.deepseek.com"
                  disabled={agentEndpointSaving}
                  onChange={(event) => setAgentBaseUrl(event.target.value)}
                  className="mt-1 w-full rounded-lg border border-[var(--cm-line)] bg-transparent px-2.5 py-1.5 font-mono text-[11px] text-neutral-800 outline-none focus:border-[var(--cm-accent-ring)] focus:ring-1 focus:ring-[var(--cm-accent-ring)] disabled:opacity-60 dark:text-neutral-100"
                />
              </label>
              <label className="mt-2 block text-[10px] text-[var(--cm-fg-muted)]">
                模型名
                <input
                  type="text"
                  value={agentModel}
                  spellCheck={false}
                  autoComplete="off"
                  placeholder="deepseek-flash"
                  disabled={agentEndpointSaving}
                  onChange={(event) => setAgentModel(event.target.value)}
                  className="mt-1 w-full rounded-lg border border-[var(--cm-line)] bg-transparent px-2.5 py-1.5 font-mono text-[11px] text-neutral-800 outline-none focus:border-[var(--cm-accent-ring)] focus:ring-1 focus:ring-[var(--cm-accent-ring)] disabled:opacity-60 dark:text-neutral-100"
                />
              </label>
              <div className="mt-2 flex items-center gap-1.5">
                <button
                  type="button"
                  onClick={handleAgentEndpointSave}
                  disabled={agentEndpointSaving}
                  className="button button--primary button--compact"
                >
                  {agentEndpointSaving ? "保存中…" : "保存地址"}
                </button>
                {agentConfig?.base_url_is_custom && (
                  <button
                    type="button"
                    onClick={handleAgentEndpointReset}
                    disabled={agentEndpointSaving}
                    className="button button--secondary button--compact"
                  >
                    恢复默认
                  </button>
                )}
              </div>
              <p className="mt-2 text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">
                任何 OpenAI 兼容服务都可以填。远程地址必须用 https；本机部署（Ollama / vLLM）可用 http。
              </p>
              {agentEndpointError && (
                <p className="mt-1.5 text-[10px] leading-relaxed text-[var(--cm-danger)]">
                  ⚠ {agentEndpointError}
                </p>
              )}
            </div>
          )}

          <p className="mt-2 text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">
            Key 只存进 macOS 钥匙串，不写进本应用的数据库、日志或崩溃报告；AI 动作发送的是你选中的那一条内容本身。
          </p>

          {/* 使用记录：只存元数据，但"我用 AI 处理过哪些内容"也是隐私，可一键抹掉。 */}
          <div className="mt-2 flex items-center gap-1.5">
            <button
              type="button"
              onClick={handleAgentRunsClear}
              disabled={agentRunsClearing}
              className="button button--secondary button--compact"
            >
              {agentRunsClearing ? "清除中…" : "清除 AI 使用记录"}
            </button>
            <span className="text-[10px] text-[var(--cm-fg-faint)]">
              {agentRunsCleared !== null && agentRunsCleared > 0
                ? `上次清除了 ${agentRunsCleared} 条`
                : "只记录用了哪条、哪个模型、耗时；不存内容本身"}
            </span>
          </div>

          {agentNotice && (
            <p className="mt-1.5 text-[10px] leading-relaxed text-[var(--cm-success)]">{agentNotice}</p>
          )}
          {agentError && (
            <p className="mt-1.5 text-[10px] leading-relaxed text-[var(--cm-danger)]">⚠ {agentError}</p>
          )}
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
              {updateStatus.notes && (
                <p className="line-clamp-2 text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">
                  {updateStatus.notes.split("\n").find((line) => line.trim())?.trim()}
                </p>
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
