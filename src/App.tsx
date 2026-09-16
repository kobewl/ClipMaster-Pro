import { useCallback, useEffect, useState } from "react";
import { SearchBar } from "@/components/SearchBar";
import { HistoryList } from "@/components/HistoryList";
import { StatusBar } from "@/components/StatusBar";
import { SettingsPanel } from "@/components/SettingsPanel";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { useClipboardHistory } from "@/hooks/useClipboardHistory";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import { commands } from "@/lib/commands";
import type { AppSettings } from "@/types/clipboard";

export default function App() {
  const [searchInput, setSearchInput] = useState("");
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [confirmClearOpen, setConfirmClearOpen] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [toastVisible, setToastVisible] = useState(false);

  const debouncedSearch = useDebouncedValue(searchInput, 200);

  const {
    items,
    total,
    loadState,
    errorMessage,
    reload,
    toggleFavorite,
    deleteItem,
    copyItem,
  } = useClipboardHistory({
    favoritesOnly,
    search: debouncedSearch,
  });

  useEffect(() => {
    commands
      .getSettings()
      .then(setSettings)
      .catch(() => {});
  }, []);

  useEffect(() => {
    function handleGlobalKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        if (settingsOpen) {
          setSettingsOpen(false);
        } else if (confirmClearOpen) {
          setConfirmClearOpen(false);
        }
      }
    }
    window.addEventListener("keydown", handleGlobalKeyDown);
    return () => window.removeEventListener("keydown", handleGlobalKeyDown);
  }, [settingsOpen, confirmClearOpen]);

  const showToast = useCallback((message: string) => {
    setToast(message);
    setToastVisible(true);
    const timer = setTimeout(() => {
      setToastVisible(false);
      setTimeout(() => setToast(null), 200);
    }, 1600);
    return () => clearTimeout(timer);
  }, []);

  const handlePaste = useCallback(
    async (id: string) => {
      try {
        await commands.pasteClipboardItem(id);
        // 窗口已由后端隐藏，不需要 toast
      } catch {
        // 粘贴失败时 fallback 到仅复制
        try {
          await copyItem(id);
          showToast("已复制到剪贴板 ✓");
        } catch {
          // useClipboardHistory 已写入 errorMessage
        }
      }
    },
    [copyItem, showToast],
  );

  const handleClearNonFavorites = useCallback(async () => {
    setConfirmClearOpen(false);
    try {
      const deleted = await commands.clearHistory(true);
      reload();
      showToast(`已清空 ${deleted} 条非收藏记录`);
    } catch {
      showToast("清空失败，请稍后重试");
    }
  }, [reload, showToast]);

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-white text-neutral-900 dark:bg-neutral-900 dark:text-neutral-100">
      <SearchBar
        value={searchInput}
        onChange={setSearchInput}
        favoritesOnly={favoritesOnly}
        onFavoritesOnlyChange={setFavoritesOnly}
        autoFocus
      />

      {errorMessage && (
        <div className="flex items-center gap-1.5 bg-red-50 px-3 py-1.5 text-xs text-red-600 dark:bg-red-900/20 dark:text-red-400">
          <span>⚠</span> {errorMessage}
        </div>
      )}

      <HistoryList
        items={items}
        loadState={loadState}
        onCopy={handlePaste}
        onToggleFavorite={toggleFavorite}
        onDelete={deleteItem}
        searchActive={debouncedSearch.trim().length > 0 || favoritesOnly}
      />

      <StatusBar
        total={total}
        captureEnabled={settings?.capture_enabled ?? true}
        onOpenSettings={() => setSettingsOpen(true)}
        onClearNonFavorites={() => setConfirmClearOpen(true)}
      />

      {/* Toast */}
      {toast && (
        <div
          className={`pointer-events-none fixed bottom-10 left-1/2 -translate-x-1/2 rounded-full bg-neutral-800/90 px-4 py-1.5 text-xs font-medium text-white shadow-lg backdrop-blur-sm transition-all duration-200 dark:bg-neutral-200/90 dark:text-neutral-900 ${
            toastVisible
              ? "translate-y-0 opacity-100"
              : "translate-y-2 opacity-0"
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
        title="清空非收藏历史"
        description="将删除所有未收藏的记录，收藏内容不受影响，此操作不可撤销。"
        confirmLabel="清空"
        onConfirm={handleClearNonFavorites}
        onCancel={() => setConfirmClearOpen(false)}
      />
    </div>
  );
}
