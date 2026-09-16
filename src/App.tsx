import { useCallback, useEffect, useState } from "react";
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

export default function App() {
  const [searchInput, setSearchInput] = useState("");
  const [activeGroupId, setActiveGroupId] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [confirmClearOpen, setConfirmClearOpen] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [toastVisible, setToastVisible] = useState(false);

  const debouncedSearch = useDebouncedValue(searchInput, 200);
  const { groups, reload: reloadGroups } = useGroups();

  const {
    items, total, loadState, errorMessage, reload,
    setItemGroup, deleteItem, copyItem,
  } = useClipboardHistory({
    groupId: activeGroupId,
    search: debouncedSearch,
  });

  useEffect(() => {
    commands.getSettings().then(setSettings).catch(() => {});
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        if (settingsOpen) setSettingsOpen(false);
        else if (confirmClearOpen) setConfirmClearOpen(false);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [settingsOpen, confirmClearOpen]);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    setToastVisible(true);
    const t = setTimeout(() => {
      setToastVisible(false);
      setTimeout(() => setToast(null), 200);
    }, 1600);
    return () => clearTimeout(t);
  }, []);

  const handlePaste = useCallback(async (id: string) => {
    try {
      await commands.pasteClipboardItem(id);
    } catch {
      try {
        await copyItem(id);
        showToast("已复制 ✓");
      } catch { /* hook handles error */ }
    }
  }, [copyItem, showToast]);

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

  const handleSetGroup = useCallback(async (id: string, gid: string | null) => {
    await setItemGroup(id, gid);
    reloadGroups();
  }, [setItemGroup, reloadGroups]);

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-white text-neutral-900 dark:bg-neutral-900 dark:text-neutral-100">
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
          <span>⚠</span> {errorMessage}
        </div>
      )}

      <HistoryList
        items={items}
        groups={groups}
        loadState={loadState}
        onPaste={handlePaste}
        onSetGroup={handleSetGroup}
        onDelete={deleteItem}
        searchActive={debouncedSearch.trim().length > 0 || activeGroupId !== null}
      />

      <StatusBar
        total={total}
        captureEnabled={settings?.capture_enabled ?? true}
        onClearHistory={() => setConfirmClearOpen(true)}
      />

      {/* Toast */}
      {toast && (
        <div className={`pointer-events-none fixed bottom-10 left-1/2 -translate-x-1/2 rounded-full bg-neutral-800/90 px-4 py-1.5 text-xs font-medium text-white shadow-lg backdrop-blur-sm transition-all duration-200 dark:bg-neutral-200/90 dark:text-neutral-900 ${
          toastVisible ? "translate-y-0 opacity-100" : "translate-y-2 opacity-0"
        }`}>
          {toast}
        </div>
      )}

      <SettingsPanel
        open={settingsOpen}
        settings={settings}
        onClose={() => setSettingsOpen(false)}
        onSave={async (next) => { const saved = await commands.updateSettings(next); setSettings(saved); }}
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
    </div>
  );
}
