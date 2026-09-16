interface StatusBarProps {
  total: number;
  captureEnabled: boolean;
  onOpenSettings: () => void;
  onClearNonFavorites: () => void;
}

export function StatusBar({
  total,
  captureEnabled,
  onOpenSettings,
  onClearNonFavorites,
}: StatusBarProps) {
  return (
    <div className="flex items-center justify-between border-t border-black/[0.06] px-3 py-1.5 text-[11px] text-neutral-400 dark:border-white/[0.08]">
      <span className="flex items-center gap-1.5">
        <span
          className={`inline-block h-1.5 w-1.5 rounded-full ${
            captureEnabled ? "bg-emerald-500" : "bg-neutral-300 dark:bg-neutral-600"
          }`}
          title={captureEnabled ? "采集中" : "采集已暂停"}
        />
        共 {total} 条{!captureEnabled && " · 已暂停"}
      </span>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onClearNonFavorites}
          className="rounded px-1.5 py-0.5 transition-colors hover:bg-black/[0.05] hover:text-neutral-600 dark:hover:bg-white/[0.08] dark:hover:text-neutral-200"
        >
          清空
        </button>
        <span className="text-neutral-200 dark:text-neutral-700">|</span>
        <button
          type="button"
          onClick={onOpenSettings}
          className="rounded px-1.5 py-0.5 transition-colors hover:bg-black/[0.05] hover:text-neutral-600 dark:hover:bg-white/[0.08] dark:hover:text-neutral-200"
        >
          ⚙ 设置
        </button>
      </div>
    </div>
  );
}
