interface StatusBarProps {
  /** 数据库里匹配当前条件的总条数。 */
  total: number;
  /** 已经渲染在列表里的条数（分页累计）。 */
  loaded: number;
  captureEnabled: boolean;
  /** 点击状态圆点即可暂停 / 恢复采集。 */
  onToggleCapture: () => void;
  onClearHistory: () => void;

  // ---- 多选 ----
  multiSelect: boolean;
  /** 已勾选条数。 */
  selectedCount: number;
  /** 勾选内容合并后的字符数（跳过图片）。 */
  selectedChars: number;
  /** 勾选的条目里有图片（合并时会跳过，要说一声）。 */
  hasImagesSelected: boolean;
  /** 全是图片 → 合并出来是空文本，复制/粘贴都不可用。 */
  canMerge: boolean;
  onEnterMultiSelect: () => void;
  onCancelMultiSelect: () => void;
  onCopyMerged: () => void;
  onPasteMerged: () => void;
}

import { Icon } from "./Icon";

/**
 * 底部状态栏。
 *
 * 多选模式下它被「批量操作条」**接管**（而不是在列表上方浮一条）：
 * 悬浮条会盖住最后几行，而多选恰恰需要看着列表勾选。
 * 操作条比常态高 10px（32 → 42），为的是放得下三个按钮；
 * 这点位移发生在一次明显的模式切换里，不会显得突兀。
 */
export function StatusBar({
  total,
  loaded,
  captureEnabled,
  onToggleCapture,
  onClearHistory,
  multiSelect,
  selectedCount,
  selectedChars,
  hasImagesSelected,
  canMerge,
  onEnterMultiSelect,
  onCancelMultiSelect,
  onCopyMerged,
  onPasteMerged,
}: StatusBarProps) {
  if (multiSelect) {
    return (
      <footer className="status-bar status-bar--batch">
        <div className="batch-info">
          <span className="batch-info__icon"><Icon name="check" /></span>
          <strong>已选 {selectedCount} 条</strong>
          {canMerge && (
            <>
              <span className="batch-sep">·</span>
              <span className="tabular-nums">共 {selectedChars} 字</span>
            </>
          )}
          {hasImagesSelected && (
            <>
              <span className="batch-sep">·</span>
              <span className="batch-note">图片会被跳过</span>
            </>
          )}
        </div>
        <div className="batch-actions">
          <button type="button" onClick={onCancelMultiSelect} className="batch-ghost">
            取消
          </button>
          <button
            type="button"
            onClick={onCopyMerged}
            disabled={!canMerge}
            title={canMerge ? "合并后复制到剪贴板" : "选中的都是图片，没法合并成文本"}
            className="button button--secondary button--compact"
          >
            复制
          </button>
          <button
            type="button"
            onClick={onPasteMerged}
            disabled={!canMerge}
            title={canMerge ? "合并后直接粘贴到刚才的应用" : "选中的都是图片，没法合并成文本"}
            className="button button--primary button--compact"
          >
            粘贴
          </button>
        </div>
      </footer>
    );
  }

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
          <span className="status-hints mr-1 hidden tabular-nums sm:inline">
            <kbd className="font-sans">↑↓</kbd> 选择 · <kbd className="font-sans">↵</kbd> 粘贴 ·{" "}
            <kbd className="font-sans">空格</kbd> 预览
          </span>
        )}
        {loaded > 0 && (
          <button
            type="button"
            onClick={onEnterMultiSelect}
            title="勾选多条，合并后一起复制"
            className="status-action status-action--multi"
          >
            <Icon name="checkSquare" />
            多选
          </button>
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
