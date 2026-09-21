import { Fragment, useCallback, useEffect, useReducer, useRef, useState } from "react";
import type { ClipboardItem, ClipGroup } from "@/types/clipboard";
import { HistoryItemRow } from "./HistoryItemRow";
import type { LoadState } from "@/hooks/useClipboardHistory";
import { Icon } from "./Icon";
import { getSourceIconPath, prefetchSourceIcons } from "@/lib/sourceIcons";
import { SEARCH_INPUT_ID } from "./SearchBar";

interface Props {
  items: ClipboardItem[];
  groups: ClipGroup[];
  loadState: LoadState;
  loadingMore: boolean;
  hasMore: boolean;
  /** 当前搜索词，用于结果高亮。 */
  keyword: string;
  /** 分组或关键词变化时用它触发「选中行回到第一条」。 */
  resetKey: string;
  /** 是否已经按某个分组过滤（过滤时不画"未分组"分隔线）。 */
  groupFilterActive: boolean;
  /** 多选模式：勾选框列 + 单击行 = 勾选。 */
  multiSelect: boolean;
  /** 已勾选的 id，按列表顺序无关（合并时按列表顺序走）。 */
  selected: ReadonlySet<string>;
  onPaste: (id: string) => void;
  onPreview: (id: string) => void;
  onTogglePick: (id: string, shiftKey: boolean) => void;
  /** 多选模式下回车 = 粘贴已选内容（合并后直接送到刚才那个应用）。 */
  onPasteMerged: () => void;
  onSelectAll: () => void;
  onSetGroup: (id: string, groupId: string | null) => void;
  onDelete: (id: string) => void;
  onLoadMore: () => void;
}

export function HistoryList({
  items,
  groups,
  loadState,
  loadingMore,
  hasMore,
  keyword,
  resetKey,
  groupFilterActive,
  multiSelect,
  selected,
  onPaste,
  onPreview,
  onTogglePick,
  onPasteMerged,
  onSelectAll,
  onSetGroup,
  onDelete,
  onLoadMore,
}: Props) {
  const [activeIndex, setActiveIndex] = useState(0);
  // 不要让每一个 HistoryItemRow 都维护自己的 setInterval。历史较多时，数百个
  // 相同的一分钟定时器会无意义地唤醒页面；列表共用一个时钟即可。
  const [now, setNow] = useState(() => Date.now());
  const listRef = useRef<HTMLUListElement>(null);
  const sentinelRef = useRef<HTMLLIElement>(null);
  // 图标是异步解析出来的：拿到新图标时用它强制重渲染一次，
  // 让已经渲染出来的行换上真实图标。
  const [, bumpIconVersion] = useReducer((version: number) => version + 1, 0);

  // 查询条件变化 → 选中行回到第一条。
  useEffect(() => {
    setActiveIndex(0);
  }, [resetKey]);

  // 列表变短（删除、刷新）时把选中行夹回有效范围。
  useEffect(() => {
    setActiveIndex((prev) => Math.min(prev, Math.max(0, items.length - 1)));
  }, [items.length]);

  useEffect(() => {
    // 对齐到下一分钟边界，避免窗口长时间打开时相对时间漂移。
    let interval: number | null = null;
    const schedule = () => {
      const delay = 60_000 - (Date.now() % 60_000);
      return window.setTimeout(() => {
        setNow(Date.now());
        interval = window.setInterval(() => setNow(Date.now()), 60_000);
      }, delay);
    };
    const timeout = schedule();
    return () => {
      window.clearTimeout(timeout);
      if (interval !== null) window.clearInterval(interval);
    };
  }, []);

  // 按应用名去重后批量解析来源图标（Rust 侧有磁盘缓存，这里只请求没查过的）。
  useEffect(() => {
    const appNames = Array.from(
      new Set(
        items
          .map((item) => item.source_app)
          .filter((name): name is string => Boolean(name)),
      ),
    );
    if (appNames.length === 0) return;

    let cancelled = false;
    prefetchSourceIcons(appNames).then((hasNewIcon) => {
      if (!cancelled && hasNewIcon) bumpIconVersion();
    });
    return () => {
      cancelled = true;
    };
  }, [items]);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      // 中文输入法组字过程中的回车是「确认候选词」，不能当成粘贴。
      if (event.isComposing || event.keyCode === 229) return;

      const targetElement = event.target as HTMLElement | null;
      const tag = targetElement?.tagName;
      const typing =
        tag === "INPUT" || tag === "TEXTAREA" || targetElement?.isContentEditable === true;

      // 搜索框里的回车 = 粘贴当前选中的那条 —— 这是本应用最核心的操作，
      // 用户的预期就是「搜到 → 回车 → 粘到刚才那个应用里」。
      // 其它输入控件（设置面板、分组弹窗）里的回车留给控件自己处理。
      const isSearchInput = targetElement?.id === SEARCH_INPUT_ID;
      const enterFromSearch = event.key === "Enter" && isSearchInput;

      // ⌘A 全选：多选模式下**必须先于输入框判断**。
      // 搜索框是自动聚焦的，如果放在下面，⌘A 会被当成「全选搜索框里的文字」而失效。
      if (multiSelect && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        onSelectAll();
        return;
      }

      // 空格预览：搜索框**空着**时也生效。
      //
      // 这是实测才发现的问题：搜索框自动聚焦，如果按「在输入框里就一律放行给输入」，
      // 那这个快捷键在默认状态下永远打不开预览 —— 只会往搜索框里打一个空格，
      // 而状态栏还写着「空格 预览」。
      //
      // 判定规则：框里有内容 = 正在打字，空格归输入框；框是空的 = 空格归预览。
      // 代价是查不了以空格开头的搜索词，那种查询本来也没有意义。
      const searchIsEmpty =
        isSearchInput && (targetElement as HTMLInputElement).value.length === 0;
      const spaceForPreview = event.key === " " && (!typing || searchIsEmpty);

      if (
        typing &&
        !enterFromSearch &&
        !spaceForPreview &&
        event.key !== "ArrowDown" &&
        event.key !== "ArrowUp"
      ) {
        return;
      }
      if (items.length === 0) return;

      if (event.key === "ArrowDown") {
        event.preventDefault();
        setActiveIndex((prev) => Math.min(prev + 1, items.length - 1));
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        setActiveIndex((prev) => Math.max(prev - 1, 0));
      } else if (spaceForPreview) {
        // 空格 = 预览（沿用 macOS Quick Look 的习惯）。
        // 多选模式下改成勾选当前行，和「单击整行 = 勾选」保持一致。
        const current = items[activeIndex];
        if (!current) return;
        event.preventDefault();
        if (multiSelect) {
          onTogglePick(current.id, false);
        } else {
          onPreview(current.id);
        }
      } else if (event.key === "Enter") {
        const selected = items[activeIndex];
        if (!selected) return;
        event.preventDefault();
        // 多选模式下回车 = 粘贴已选内容，由 App 决定粘什么。
        if (multiSelect) onPasteMerged();
        else onPaste(selected.id);
      }
    },
    [
      items,
      activeIndex,
      onPaste,
      onPreview,
      multiSelect,
      onTogglePick,
      onSelectAll,
      onPasteMerged,
    ],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  // 滚动到底部的哨兵：露出即续拉下一页。
  useEffect(() => {
    const sentinel = sentinelRef.current;
    const root = listRef.current;
    if (!sentinel || !root || !hasMore) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) onLoadMore();
      },
      { root, rootMargin: "160px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, onLoadMore, items.length]);

  if (loadState === "loading" && items.length === 0) {
    return <ListSkeleton />;
  }

  if (loadState === "error" && items.length === 0) {
    return (
      <div className="empty-state text-[var(--cm-danger)]">
        <span className="empty-state__icon"><Icon name="alert" /></span>
        <strong>加载失败</strong>
        <span>请稍后再试</span>
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <div className="empty-state">
        <span className="empty-state__icon"><Icon name={keyword.trim() ? "search" : "clipboard"} /></span>
        <strong>{keyword.trim() ? "没有找到匹配的记录" : "开始收集你的剪贴板"}</strong>
        {!keyword.trim() && (
          <span>复制的文本、链接和图片会自动出现在这里</span>
        )}
      </div>
    );
  }

  // 后端把已分组的条目排在前面，这里在两种内容的分界处补一条分隔线。
  const firstUngrouped = items.findIndex((item) => item.group_id === null);
  const showDivider = !groupFilterActive && firstUngrouped > 0;

  return (
    <ul
      ref={listRef}
      role="listbox"
      aria-label="剪贴板历史"
      className="history-list scrollbar-thin"
    >
      {items.map((item, index) => (
        <Fragment key={item.id}>
          {showDivider && index === firstUngrouped && (
            <li
              aria-hidden
              className="list-divider"
            >
              <span className="shrink-0">未分组</span>
              <span className="h-px flex-1 bg-[var(--cm-line)]" />
            </li>
          )}
          <HistoryItemRow
            item={item}
            active={index === activeIndex}
            now={now}
            groups={groups}
            keyword={keyword}
            iconSrc={getSourceIconPath(item.source_app)}
            multiSelect={multiSelect}
            picked={selected.has(item.id)}
            onPaste={onPaste}
            onPreview={onPreview}
            onTogglePick={onTogglePick}
            onSetGroup={onSetGroup}
            onDelete={onDelete}
          />
        </Fragment>
      ))}

      <li
        ref={sentinelRef}
        aria-hidden
        className="px-3 py-2 text-center text-[11px] text-[var(--cm-fg-faint)]"
      >
        {loadingMore ? (
          <span className="animate-pulse">加载中…</span>
        ) : hasMore ? (
          <button
            type="button"
            onClick={onLoadMore}
            className="rounded px-2 py-0.5 transition-colors hover:bg-[var(--cm-hover)] hover:text-[var(--cm-fg-muted)] dark:hover:bg-white/[0.06]"
          >
            加载更多
          </button>
        ) : (
          "— 到底了 —"
        )}
      </li>
    </ul>
  );
}

/** 首屏骨架屏，比一句"加载中"更接近最终布局，视觉上不跳动。 */
function ListSkeleton() {
  return (
    <div className="flex-1 space-y-1 px-1.5 py-1" aria-label="加载中" aria-busy>
      {Array.from({ length: 7 }, (_, index) => (
        <div key={index} className="flex flex-col gap-1.5 rounded-lg px-3 py-2">
          <div className="flex justify-between">
            <span className="h-2.5 w-24 animate-pulse rounded bg-[var(--cm-line)]" />
            <span className="h-2.5 w-12 animate-pulse rounded bg-[var(--cm-line)]" />
          </div>
          <span className="h-3 w-full animate-pulse rounded bg-[var(--cm-hover)]" />
          <span
            className="h-3 animate-pulse rounded bg-[var(--cm-hover)]"
            style={{ width: `${45 + ((index * 13) % 40)}%` }}
          />
        </div>
      ))}
    </div>
  );
}
