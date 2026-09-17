import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { SearchBar } from "@/components/SearchBar";
import { GroupBar } from "@/components/GroupBar";
import { HistoryList } from "@/components/HistoryList";
import { StatusBar } from "@/components/StatusBar";
import { SettingsPanel } from "@/components/SettingsPanel";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { useClipboardHistory } from "@/hooks/useClipboardHistory";
import { useGroups } from "@/hooks/useGroups";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import { commands } from "@/lib/commands";
import type { AppSettings } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";

export default function App() {
  const [searchInput, setSearchInput] = useState("");
  const [activeGroupId, setActiveGroupId] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [confirmClearOpen, setConfirmClearOpen] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [toastVisible, setToastVisible] = useState(false);

  const toastTimerRef = useRef<number | null>(null);
  const toastFadeTimerRef = useRef<number | null>(null);

  const debouncedSearch = useDebouncedValue(searchInput, 200);
  const { groups, reload: reloadGroups } = useGroups();

  const {
    items,
    total,
    loadState,
    loadingMore,
    hasMore,
    errorMessage,
    clearError,
    reload,
    loadMore,
    setItemGroup,
    deleteItem,
    copyItem,
  } = useClipboardHistory({
    groupId: activeGroupId,
    search: debouncedSearch,
  });

  useEffect(() => {
    commands.getSettings().then(setSettings).catch(() => {});
  }, []);

  // 选中的分组被删掉时，自动退回「全部」，否则会停在一个永远为空的列表上。
  useEffect(() => {
    if (activeGroupId === null) return;
    if (groups.some((group) => group.id === activeGroupId)) return;
    setActiveGroupId(null);
  }, [groups, activeGroupId]);

  /** 统一的提示条：后来的提示会顶掉前一个，不会出现"前一个的定时器把新的关掉"。 */
  const showToast = useCallback((message: string, durationMs = 1600) => {
    if (toastTimerRef.current !== null) window.clearTimeout(toastTimerRef.current);
    if (toastFadeTimerRef.current !== null) window.clearTimeout(toastFadeTimerRef.current);

    setToast(message);
    setToastVisible(true);
    toastTimerRef.current = window.setTimeout(() => {
      setToastVisible(false);
      toastFadeTimerRef.current = window.setTimeout(() => setToast(null), 200);
    }, durationMs);
  }, []);

  useEffect(() => {
    return () => {
      if (toastTimerRef.current !== null) window.clearTimeout(toastTimerRef.current);
      if (toastFadeTimerRef.current !== null) window.clearTimeout(toastFadeTimerRef.current);
    };
  }, []);

  // Esc 的优先级：设置面板 → 清空确认 → 清空搜索词。
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      if (settingsOpen) {
        setSettingsOpen(false);
      } else if (confirmClearOpen) {
        setConfirmClearOpen(false);
      } else if (searchInput.length > 0) {
        setSearchInput("");
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [settingsOpen, confirmClearOpen, searchInput]);

  /**
   * 一键粘贴：先写入剪贴板并模拟 ⌘V（窗口会自动隐藏）。
   *
   * 失败要分两种，处理方式完全不同：
   * - `paste_failed`：内容**已经写进剪贴板了**，只是模拟 ⌘V 没成功（几乎都是没给
   *   「辅助功能」权限）。此时窗口已经隐藏，得先把它叫回来，否则用户什么都看不到 ——
   *   这正是「点了没反应」的来源。
   * - 其它错误：连写剪贴板都没成功，退化成「只复制」。
   */
  const handlePaste = useCallback(
    async (id: string) => {
      try {
        await commands.pasteClipboardItem(id);
      } catch (error) {
        if (isCommandError(error) && error.code === "paste_failed") {
          try {
            const currentWindow = getCurrentWindow();
            await currentWindow.show();
            await currentWindow.setFocus();
          } catch {
            /* 窗口控制失败不影响提示本身 */
          }
          showToast(error.message, 6000);
          return;
        }
        try {
          await copyItem(id);
          showToast("已复制 ✓");
        } catch {
          /* 失败提示由 hook 统一写入 errorMessage */
        }
      }
    },
    [copyItem, showToast],
  );

  const handleSetGroup = useCallback(
    async (id: string, groupId: string | null) => {
      await setItemGroup(id, groupId);
      reloadGroups();
    },
    [setItemGroup, reloadGroups],
  );

  const handleDelete = useCallback(
    async (id: string) => {
      await deleteItem(id);
      reloadGroups();
    },
    [deleteItem, reloadGroups],
  );

  const handleClear = useCallback(async () => {
    setConfirmClearOpen(false);
    try {
      const deleted = await commands.clearHistory(true);
      reload();
      reloadGroups();
      showToast(`已清空 ${deleted} 条未分组记录`);
    } catch {
      showToast("清空失败");
    }
  }, [reload, reloadGroups, showToast]);

  const handleToggleCapture = useCallback(async () => {
    if (!settings) return;
    try {
      const next = await commands.setCaptureEnabled(!settings.capture_enabled);
      setSettings(next);
      showToast(next.capture_enabled ? "已恢复采集" : "已暂停采集");
    } catch {
      showToast("操作失败");
    }
  }, [settings, showToast]);

  const trimmedSearch = debouncedSearch.trim();

  return (
    <main className="app-shell">
      <section className="app-surface">
        <SearchBar
          value={searchInput}
          onChange={setSearchInput}
          onOpenSettings={() => setSettingsOpen(true)}
          autoFocus
        />

        <GroupBar
          groups={groups}
          activeGroupId={activeGroupId}
          onSelect={setActiveGroupId}
          onGroupsReload={reloadGroups}
        />

      {errorMessage && (
        <div className="flex items-center gap-1.5 bg-red-50 px-3 py-1 text-[11px] text-red-600 dark:bg-red-900/20 dark:text-red-400">
          <span>⚠</span>
          <span className="min-w-0 flex-1 truncate">{errorMessage}</span>
          <button
            type="button"
            onClick={clearError}
            aria-label="关闭提示"
            className="shrink-0 rounded px-1 leading-none transition-colors hover:bg-red-500/10"
          >
            ✕
          </button>
        </div>
      )}

        <HistoryList
          items={items}
          groups={groups}
          loadState={loadState}
          loadingMore={loadingMore}
          hasMore={hasMore}
          keyword={trimmedSearch}
          resetKey={`${activeGroupId ?? "all"}::${trimmedSearch}`}
          groupFilterActive={activeGroupId !== null}
          onPaste={handlePaste}
          onSetGroup={handleSetGroup}
          onDelete={handleDelete}
          onLoadMore={loadMore}
        />

        <StatusBar
          total={total}
          loaded={items.length}
          captureEnabled={settings?.capture_enabled ?? true}
          onToggleCapture={handleToggleCapture}
          onClearHistory={() => setConfirmClearOpen(true)}
        />
      </section>

      {toast && (
        <div
          role="status"
          aria-live="polite"
          className={`cm-fade-in pointer-events-none fixed bottom-10 left-1/2 max-w-[min(560px,calc(100%-32px))] -translate-x-1/2 rounded-2xl bg-neutral-800/90 px-4 py-2 text-center text-xs font-medium leading-relaxed text-white shadow-lg backdrop-blur-sm transition-all duration-200 dark:bg-neutral-200/90 dark:text-neutral-900 ${
            toastVisible ? "translate-y-0 opacity-100" : "translate-y-2 opacity-0"
          }`}
        >
          {toast}
        </div>
      )}

      <SettingsPanel
        open={settingsOpen}
        settings={settings}
        onClose={() => setSettingsOpen(false)}
        onSave={async (next) => {
          const saved = await commands.updateSettings(next);
          setSettings(saved);
        }}
        onSettingsChange={setSettings}
      />

      <ConfirmDialog
        open={confirmClearOpen}
        title="清空未分组历史"
        description="将删除所有未分组的记录，已分组内容不受影响。此操作不可撤销。"
        confirmLabel="清空"
        onConfirm={handleClear}
        onCancel={() => setConfirmClearOpen(false)}
      />
    </main>
  );
}
