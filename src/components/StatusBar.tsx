interface StatusBarProps {
  /** 数据库里匹配当前条件的总条数。 */
  total: number;
  /** 已经渲染在列表里的条数（分页累计）。 */
  loaded: number;
  captureEnabled: boolean;
  /** 点击状态圆点即可暂停 / 恢复采集。 */
  onToggleCapture: () => void;
  onClearHistory: () => void;
}

import { Icon } from "./Icon";

export function StatusBar({
  total,
  loaded,
  captureEnabled,
  onToggleCapture,
  onClearHistory,
}: StatusBarProps) {
  return (
    <footer className="status-bar">
      <button
        type="button"
        onClick={onToggleCapture}
        title={captureEnabled ? "点击暂停采集" : "点击恢复采集"}
        className="capture-status"
      >
        <span
          className={`inline-block h-1.5 w-1.5 rounded-full transition-colors ${
            captureEnabled ? "bg-emerald-500" : "bg-neutral-300 dark:bg-neutral-600"
          }`}
        />
        <span>{captureEnabled ? "正在采集" : "已暂停"}</span>
        <span className="status-separator" />
        共 {total} 条
        {loaded < total && ` · 已显示 ${loaded}`}
        <Icon name={captureEnabled ? "pause" : "play"} />
      </button>

      <div className="flex items-center gap-1">
        {loaded > 0 && (
          <span className="mr-1 hidden tabular-nums sm:inline">
            <kbd className="font-sans">↑↓</kbd> 选择 · <kbd className="font-sans">↵</kbd> 粘贴
          </span>
        )}
        <button
          type="button"
          onClick={onClearHistory}
          className="status-action"
        >
          清空未分组
        </button>
      </div>
    </footer>
  );
}
