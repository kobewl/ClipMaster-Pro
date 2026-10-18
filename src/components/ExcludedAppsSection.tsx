import { useState } from "react";
import type { AppSettings, SourceApp } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";
import { commands } from "@/lib/commands";

/** 默认名单里那几个 bundle id 的友好名，免得用户只看到一串反向域名。 */
const KNOWN_APP_LABELS: Record<string, string> = {
  "com.apple.keychainaccess": "钥匙串访问",
  "com.1password.1password": "1Password",
  "com.agilebits.onepassword7": "1Password 7",
  "com.bitwarden.desktop": "Bitwarden",
  "com.apple.passwords": "密码",
};

const INPUT_CLASS =
  "w-full rounded-lg border border-[var(--cm-line)] bg-transparent px-2.5 py-1.5 font-mono text-[11px] text-neutral-800 outline-none focus:border-[var(--cm-accent-ring)] focus:ring-1 focus:ring-[var(--cm-accent-ring)] disabled:opacity-60 dark:text-neutral-100";

interface Props {
  excludedApps: string[];
  onSettingsChange: (settings: AppSettings) => void;
}

/**
 * 隐私：「不记录这些应用」名单。每次增删都立即落库并生效，不走面板底部的「保存」。
 * 条目可以是 bundle id，也可以是应用名（大小写不敏感匹配）。
 */
export function ExcludedAppsSection({ excludedApps, onSettingsChange }: Props) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [manualInput, setManualInput] = useState("");
  const [pickerOpen, setPickerOpen] = useState(false);
  /** null = 还没读取（或读取中）。只在展开时才去读。 */
  const [recentApps, setRecentApps] = useState<SourceApp[] | null>(null);
  const [recentError, setRecentError] = useState<string | null>(null);

  const excludedLower = new Set(excludedApps.map((app) => app.toLowerCase()));

  function labelFor(entry: string): string {
    const lower = entry.toLowerCase();
    const known = KNOWN_APP_LABELS[lower];
    if (known) return known;
    const recent = recentApps?.find((app) => app.bundle_id?.toLowerCase() === lower);
    return recent?.name ?? entry;
  }

  async function save(next: string[]) {
    setSaving(true);
    setError(null);
    try {
      const updated = await commands.setExcludedApps(next);
      onSettingsChange(updated);
      return true;
    } catch (err: unknown) {
      setError(isCommandError(err) ? err.message : "保存不记录的应用失败");
      return false;
    } finally {
      setSaving(false);
    }
  }

  function handleRemove(entry: string) {
    void save(excludedApps.filter((app) => app !== entry));
  }

  function handleAdd(value: string) {
    const trimmed = value.trim();
    if (!trimmed) return Promise.resolve(false);
    if (excludedLower.has(trimmed.toLowerCase())) {
      setError(null);
      return Promise.resolve(true);
    }
    return save([...excludedApps, trimmed]);
  }

  async function handleManualAdd() {
    if (await handleAdd(manualInput)) setManualInput("");
  }

  function togglePicker() {
    const next = !pickerOpen;
    setPickerOpen(next);
    if (!next) return;
    setRecentError(null);
    commands
      .listRecentSourceApps()
      .then(setRecentApps)
      .catch((err: unknown) =>
        setRecentError(isCommandError(err) ? err.message : "读取最近来源应用失败"),
      );
  }

  const candidates = (recentApps ?? []).filter((app) => {
    const value = app.bundle_id ?? app.name;
    if (!value) return false;
    return !excludedLower.has(value.toLowerCase()) &&
      !(app.name && excludedLower.has(app.name.toLowerCase()));
  });

  return (
    <fieldset className="settings-section mt-3">
      <legend className="px-1.5 text-[11px] font-medium text-[var(--cm-fg-faint)]">隐私</legend>
      <p className="text-[10px] leading-relaxed text-[var(--cm-fg-faint)]">
        从这些应用复制的内容不会被记录，按 Bundle ID 或应用名匹配。密码管理器标记为隐藏 / 临时的内容始终不记录。
      </p>

      {excludedApps.length === 0 ? (
        <p className="mt-2 text-xs text-[var(--cm-fg-muted)]">名单为空，所有应用的复制都会被记录</p>
      ) : (
        <ul aria-label="不记录的应用" className="mt-2 space-y-1.5">
          {excludedApps.map((entry) => {
            const label = labelFor(entry);
            return (
              <li key={entry} className="flex items-center justify-between gap-2">
                <span className="min-w-0 text-xs text-[var(--cm-fg-muted)]">
                  {label}
                  {label !== entry && (
                    <span className="ml-1.5 break-all font-mono text-[10px] text-[var(--cm-fg-faint)]">
                      {entry}
                    </span>
                  )}
                </span>
                <button
                  type="button"
                  className="button button--secondary button--compact"
                  onClick={() => handleRemove(entry)}
                  disabled={saving}
                  aria-label={`移除 ${label}`}
                >
                  移除
                </button>
              </li>
            );
          })}
        </ul>
      )}

      <div className="mt-3">
        <button
          type="button"
          className="button button--secondary button--compact"
          onClick={togglePicker}
          aria-expanded={pickerOpen}
        >
          从最近复制来源中添加
        </button>
        {pickerOpen && (
          <div className="mt-2">
            {recentError ? (
              <p role="alert" className="text-[10px] leading-relaxed text-[var(--cm-danger)]">
                ⚠ {recentError}
              </p>
            ) : recentApps === null ? (
              <p className="text-[10px] text-[var(--cm-fg-faint)]">读取中…</p>
            ) : candidates.length === 0 ? (
              <p className="text-[10px] text-[var(--cm-fg-faint)]">最近没有可添加的来源应用</p>
            ) : (
              <ul aria-label="最近复制来源" className="flex flex-wrap gap-1.5">
                {candidates.map((app) => {
                  const value = (app.bundle_id ?? app.name) as string;
                  const name = app.name ?? value;
                  return (
                    <li key={value}>
                      <button
                        type="button"
                        className="button button--secondary button--compact"
                        onClick={() => void handleAdd(value)}
                        disabled={saving}
                        aria-label={`不记录 ${name}`}
                        title={value}
                      >
                        {name}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        )}
      </div>

      <div className="mt-3 flex items-end gap-2">
        <label className="block min-w-0 flex-1 text-xs text-[var(--cm-fg-muted)]">
          手动添加
          <input
            type="text"
            value={manualInput}
            onChange={(event) => setManualInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                // 保存中不重复提交。输入框本身不禁用：禁用会把键盘焦点踢出去。
                if (!saving) void handleManualAdd();
              }
            }}
            placeholder="Bundle ID 或应用名，如 com.example.app"
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            className={`mt-1 ${INPUT_CLASS}`}
          />
        </label>
        <button
          type="button"
          className="button button--secondary button--compact"
          onClick={() => void handleManualAdd()}
          disabled={saving || manualInput.trim().length === 0}
        >
          添加
        </button>
      </div>

      {error && (
        <p role="alert" className="mt-1.5 text-[10px] leading-relaxed text-[var(--cm-danger)]">
          ⚠ {error}
        </p>
      )}
    </fieldset>
  );
}
