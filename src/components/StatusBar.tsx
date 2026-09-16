interface StatusBarProps {
  total: number;
  captureEnabled: boolean;
  onClearHistory: () => void;
}

export function StatusBar({
  total,
  captureEnabled,
  onClearHistory,
}: StatusBarProps) {
  return (
    <div className="flex items-center justify-between border-t border-black/[0.05] px-3 py-1 text-[11px] text-neutral-400 dark:border-white/[0.06]">
      <span className="flex items-center gap-1.5">
        <span
          className={`inline-block h-1.5 w-1.5 rounded-full ${
            captureEnabled ? "bg-emerald-500" : "bg-neutral-300 dark:bg-neutral-600"
          }`}
          title={captureEnabled ? "采集中" : "已暂停"}
        />
        共 {total} 条{!captureEnabled && " · 已暂停"}
      </span>
      <button
        type="button"
        onClick={onClearHistory}
        className="rounded px-1.5 py-0.5 transition-colors hover:bg-black/[0.05] hover:text-neutral-600 dark:hover:bg-white/[0.08] dark:hover:text-neutral-300"
      >
        清空
      </button>
    </div>
  );
}
