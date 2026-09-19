import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { SearchBar } from "@/components/SearchBar";
import { Toolbar } from "@/components/Toolbar";
import { HistoryList } from "@/components/HistoryList";
import { StatusBar } from "@/components/StatusBar";
import { SettingsPanel } from "@/components/SettingsPanel";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { PreviewDialog } from "@/components/PreviewDialog";
import { useClipboardHistory } from "@/hooks/useClipboardHistory";
import { useGroups } from "@/hooks/useGroups";
import { useDebouncedValue } from "@/hooks/useDebouncedValue";
import { useTrayEvents } from "@/hooks/useTrayEvents";
import { commands } from "@/lib/commands";
import { mergeSelectedItems } from "@/lib/mergeItems";
import { getSourceIconPath } from "@/lib/sourceIcons";
import type { AppSettings } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";

export default function App() {
  const [searchInput, setSearchInput] = useState("");
  const [activeGroupId, setActiveGroupId] = useState<string | null>(null);
  /** 列表筛选：内容类型与时间档（null = 全部）。 */
  const [filterType, setFilterType] = useState<string | null>(null);
  const [filterTimeRange, setFilterTimeRange] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [confirmClearOpen, setConfirmClearOpen] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [toastVisible, setToastVisible] = useState(false);
  /** 正在查看全部内容的那一条（null = 弹窗关着）。 */
  const [previewId, setPreviewId] = useState<string | null>(null);
  /** 多选模式。 */
  const [multiSelect, setMultiSelect] = useState(false);
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(() => new Set());

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
    contentType: filterType,
    timeRange: filterTimeRange,
  });

  useEffect(() => {
    commands.getSettings().then(setSettings).catch(() => {});
  }, []);

  // 托盘菜单 → 界面联动：采集开关变化（含托盘自己切的）同步到状态栏与
  // 设置面板；「打开设置」时窗口已被后端拉起并聚焦，这里只负责开面板。
  useTrayEvents({
    onCaptureChanged: (capture_enabled) =>
      setSettings((prev) => (prev ? { ...prev, capture_enabled } : prev)),
    onOpenSettings: () => setSettingsOpen(true),
  });

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

  /**
   * 粘贴失败时的统一收尾。「一键粘贴」和「合并后粘贴」共用，
   * 免得两条路径给出不一样的提示。
   */
  const recoverFromPasteFailure = useCallback(
    async (error: unknown, fallbackCopy: () => Promise<void>) => {
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
        await fallbackCopy();
        showToast("已复制 ✓");
      } catch {
        /* 失败提示由 hook 统一写入 errorMessage */
      }
    },
    [showToast],
  );

  // -------------------------------------------------------------------------
  //  多选：选择集与合并
  // -------------------------------------------------------------------------

  const exitMultiSelect = useCallback(() => {
    setMultiSelect(false);
    setSelectedIds(new Set());
  }, []);

  const enterMultiSelect = useCallback((seedId?: string) => {
    setMultiSelect(true);
    setSelectedIds(seedId ? new Set([seedId]) : new Set());
  }, []);

  /**
   * 勾选 / 取消一条。
   *
   * Shift 点击做「连选」：从上次点的那条一路选到这一条。多选场景下用户
   * 十有八九是要一段连续区间（几段相邻的调研结论），逐个点太费事。
   */
  const lastPickedRef = useRef<string | null>(null);
  const togglePick = useCallback(
    (id: string, shiftKey: boolean) => {
      setSelectedIds((prev) => {
        const next = new Set(prev);
        const anchor = lastPickedRef.current;

        if (shiftKey && anchor && anchor !== id && items.some((it) => it.id === anchor)) {
          const from = items.findIndex((it) => it.id === anchor);
          const to = items.findIndex((it) => it.id === id);
          const [start, end] = from < to ? [from, to] : [to, from];
          for (let i = start; i <= end; i += 1) next.add(items[i].id);
        } else if (next.has(id)) {
          next.delete(id);
        } else {
          next.add(id);
        }

        return next;
      });
      lastPickedRef.current = id;
    },
    [items],
  );

  /** 全选当前已加载的条目（⌘A）。 */
  const selectAll = useCallback(() => {
    setSelectedIds(new Set(items.map((item) => item.id)));
  }, [items]);

  /**
   * 合并选中的内容：只拼文本条目，图片 / HTML / 文件按规则跳过。
   *
   * `skipped` 不直接用 —— 界面上的提示是「选了非文本内容就会被跳过」，
   * 而不是「跳过了 N 条」，所以只需要知道有没有。
   */
  const merged = useMemo(() => {
    if (!multiSelect || selectedIds.size === 0) {
      return { text: "", hasNonText: false };
    }
    const result = mergeSelectedItems(items, selectedIds);
    return { text: result.text, hasNonText: result.skipped > 0 };
  }, [items, selectedIds, multiSelect]);

  const canMerge = merged.text.length > 0;

  /** 预览弹窗要显示的那一条。列表刷新后它可能已经不在了，这时弹窗自动关闭。 */
  const previewItem = useMemo(
    () => (previewId === null ? null : items.find((item) => item.id === previewId) ?? null),
    [previewId, items],
  );

  const handleCopyMerged = useCallback(async () => {
    if (!canMerge) return;
    try {
      await commands.copyTextToClipboard(merged.text);
      exitMultiSelect();
      showToast(`已复制 ${selectedIds.size} 条 ✓`);
    } catch {
      showToast("合并复制失败");
    }
  }, [canMerge, merged.text, exitMultiSelect, showToast, selectedIds.size]);

  const handlePasteMerged = useCallback(async () => {
    if (!canMerge) return;
    try {
      await commands.pasteText(merged.text);
      exitMultiSelect();
    } catch (error) {
      await recoverFromPasteFailure(error, async () => {
        await commands.copyTextToClipboard(merged.text);
      });
      exitMultiSelect();
    }
  }, [canMerge, merged.text, exitMultiSelect, recoverFromPasteFailure]);

  // Esc 的优先级：预览弹窗 → 多选 → 设置面板 → 清空确认 → 清空搜索词。
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      if (previewId !== null) {
        setPreviewId(null);
      } else if (multiSelect) {
        exitMultiSelect();
      } else if (settingsOpen) {
        setSettingsOpen(false);
      } else if (confirmClearOpen) {
        setConfirmClearOpen(false);
      } else if (searchInput.length > 0) {
        setSearchInput("");
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [settingsOpen, confirmClearOpen, searchInput, previewId, multiSelect, exitMultiSelect]);

  // 列表刷新（删除、换分组）后把已不存在的 id 从选择集里摘掉，
  // 否则底部会显示「已选 3 条」而列表里只有 2 条被勾上。
  useEffect(() => {
    if (!multiSelect) return;
    const alive = new Set(items.map((item) => item.id));
    setSelectedIds((prev) => {
      const next = new Set([...prev].filter((id) => alive.has(id)));
      return next.size === prev.size ? prev : next;
    });
  }, [items, multiSelect]);

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
        await recoverFromPasteFailure(error, () => copyItem(id));
      }
    },
    [copyItem, recoverFromPasteFailure],
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

        <Toolbar
          groups={groups}
          activeGroupId={activeGroupId}
          onSelect={setActiveGroupId}
          onGroupsReload={reloadGroups}
          contentType={filterType}
          timeRange={filterTimeRange}
          onContentTypeChange={setFilterType}
          onTimeRangeChange={setFilterTimeRange}
        />

      {errorMessage && (
        <div className="flex items-center gap-1.5 bg-[color-mix(in_srgb,var(--cm-danger)_9%,transparent)] px-3 py-1 text-[11px] text-[var(--cm-danger)]">
          <span>⚠</span>
          <span className="min-w-0 flex-1 truncate">{errorMessage}</span>
          <button
            type="button"
            onClick={clearError}
            aria-label="关闭提示"
            className="shrink-0 rounded px-1 leading-none transition-colors hover:bg-[color-mix(in_srgb,var(--cm-danger)_12%,transparent)]"
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
          multiSelect={multiSelect}
          selected={selectedIds}
          onPaste={handlePaste}
          onPreview={setPreviewId}
          onTogglePick={togglePick}
          onPasteMerged={handlePasteMerged}
          onSelectAll={selectAll}
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
          multiSelect={multiSelect}
          selectedCount={selectedIds.size}
          selectedChars={merged.text.length}
          hasNonTextSelected={merged.hasNonText}
          canMerge={canMerge}
          onEnterMultiSelect={() => enterMultiSelect()}
          onCancelMultiSelect={exitMultiSelect}
          onCopyMerged={handleCopyMerged}
          onPasteMerged={handlePasteMerged}
        />
      </section>

      {toast && (
        <div
          role="status"
          aria-live="polite"
          className={`cm-fade-in pointer-events-none fixed bottom-10 left-1/2 max-w-[min(560px,calc(100%-32px))] -translate-x-1/2 rounded-2xl px-4 py-2 text-center text-xs font-medium leading-relaxed text-[var(--cm-fg)] shadow-lg backdrop-blur-sm transition-all duration-200 ${
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

      <PreviewDialog
        item={previewItem}
        iconSrc={getSourceIconPath(previewItem?.source_app ?? null)}
        onCopy={async (id) => {
          try {
            await copyItem(id);
            setPreviewId(null);
            showToast("已复制 ✓");
          } catch {
            /* 失败提示由 hook 统一写入 errorMessage */
          }
        }}
        onPaste={(id) => {
          setPreviewId(null);
          handlePaste(id);
        }}
        onClose={() => setPreviewId(null)}
      />
    </main>
  );
}
