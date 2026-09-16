import { useCallback, useEffect, useRef, useState } from "react";
import type { ClipboardItem } from "@/types/clipboard";
import { HistoryItemRow } from "./HistoryItemRow";
import type { LoadState } from "@/hooks/useClipboardHistory";

interface HistoryListProps {
  items: ClipboardItem[];
  loadState: LoadState;
  onCopy: (id: string) => void;
  onToggleFavorite: (id: string) => void;
  onDelete: (id: string) => void;
  searchActive: boolean;
}

export function HistoryList({
  items,
  loadState,
  onCopy,
  onToggleFavorite,
  onDelete,
  searchActive,
}: HistoryListProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    setActiveIndex(0);
  }, [items.length, searchActive]);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      // 如果焦点在 input 或 button 上（设置面板、搜索框等），不拦截方向键
      const tag = (event.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") {
        if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      }

      if (items.length === 0) return;

      if (event.key === "ArrowDown") {
        event.preventDefault();
        setActiveIndex((prev) => Math.min(prev + 1, items.length - 1));
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        setActiveIndex((prev) => Math.max(prev - 1, 0));
      } else if (event.key === "Enter") {
        const target = items[activeIndex];
        if (target) {
          event.preventDefault();
          onCopy(target.id);
        }
      }
    },
    [items, activeIndex, onCopy],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  if (loadState === "loading" && items.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-neutral-400">
        <span className="animate-pulse">正在加载…</span>
      </div>
    );
  }

  if (loadState === "error") {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center text-sm text-red-400">
        <span className="text-2xl">⚠️</span>
        加载历史记录失败，请稍后重试
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center text-sm text-neutral-400">
        <span className="text-3xl opacity-40">
          {searchActive ? "🔍" : "📋"}
        </span>
        {searchActive ? "没有找到匹配的记录" : "暂无剪贴板历史"}
        {!searchActive && (
          <span className="text-[11px]">复制内容后会自动出现在这里</span>
        )}
      </div>
    );
  }

  return (
    <ul
      ref={listRef}
      role="listbox"
      aria-label="剪贴板历史记录"
      className="scrollbar-thin flex-1 space-y-px overflow-y-auto px-1.5 py-1.5"
    >
      {items.map((item, index) => (
        <HistoryItemRow
          key={item.id}
          item={item}
          active={index === activeIndex}
          onClick={() => {
            setActiveIndex(index);
            onCopy(item.id);
          }}
          onToggleFavorite={() => onToggleFavorite(item.id)}
          onDelete={() => onDelete(item.id)}
        />
      ))}
    </ul>
  );
}
