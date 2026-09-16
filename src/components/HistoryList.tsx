import { useCallback, useEffect, useRef, useState } from "react";
import type { ClipboardItem, ClipGroup } from "@/types/clipboard";
import { HistoryItemRow } from "./HistoryItemRow";
import type { LoadState } from "@/hooks/useClipboardHistory";

interface Props {
  items: ClipboardItem[];
  groups: ClipGroup[];
  loadState: LoadState;
  onPaste: (id: string) => void;
  onSetGroup: (id: string, groupId: string | null) => void;
  onDelete: (id: string) => void;
  searchActive: boolean;
}

export function HistoryList({
  items,
  groups,
  loadState,
  onPaste,
  onSetGroup,
  onDelete,
  searchActive,
}: Props) {
  const [activeIndex, setActiveIndex] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => { setActiveIndex(0); }, [items.length, searchActive]);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      const tag = (event.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") {
        if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      }
      if (items.length === 0) return;
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setActiveIndex((p) => Math.min(p + 1, items.length - 1));
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        setActiveIndex((p) => Math.max(p - 1, 0));
      } else if (event.key === "Enter") {
        const target = items[activeIndex];
        if (target) { event.preventDefault(); onPaste(target.id); }
      }
    },
    [items, activeIndex, onPaste],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  if (loadState === "loading" && items.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-neutral-400">
        <span className="animate-pulse">加载中…</span>
      </div>
    );
  }

  if (loadState === "error") {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 text-sm text-red-400">
        <span className="text-2xl">⚠️</span>加载失败
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-1.5 text-sm text-neutral-400">
        <span className="text-3xl opacity-40">{searchActive ? "🔍" : "📋"}</span>
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
      aria-label="剪贴板历史"
      className="scrollbar-thin flex-1 space-y-px overflow-y-auto px-1.5 py-1"
    >
      {items.map((item, idx) => (
        <HistoryItemRow
          key={item.id}
          item={item}
          active={idx === activeIndex}
          groups={groups}
          onClick={() => { setActiveIndex(idx); onPaste(item.id); }}
          onSetGroup={(gid) => onSetGroup(item.id, gid)}
          onDelete={() => onDelete(item.id)}
        />
      ))}
    </ul>
  );
}
