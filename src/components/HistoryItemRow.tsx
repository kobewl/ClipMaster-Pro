import { memo, useEffect, useRef } from "react";
import type { ClipboardItem } from "@/types/clipboard";

interface HistoryItemRowProps {
  item: ClipboardItem;
  active: boolean;
  onClick: () => void;
  onToggleFavorite: () => void;
  onDelete: () => void;
}

function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";

  const now = new Date();
  const diff = now.getTime() - date.getTime();
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);

  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  if (hours < 24) return `${hours} 小时前`;

  return date.toLocaleString(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export const HistoryItemRow = memo(function HistoryItemRow({
  item,
  active,
  onClick,
  onToggleFavorite,
  onDelete,
}: HistoryItemRowProps) {
  const ref = useRef<HTMLLIElement>(null);

  useEffect(() => {
    if (active && ref.current) {
      ref.current.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }, [active]);

  return (
    <li
      ref={ref}
      role="option"
      aria-selected={active}
      onClick={onClick}
      className={`group flex cursor-pointer items-start gap-2 rounded-lg px-3 py-2 text-sm transition-colors ${
        active
          ? "bg-blue-500/15 ring-1 ring-blue-500/20 dark:bg-blue-400/20 dark:ring-blue-400/20"
          : "hover:bg-black/[0.04] dark:hover:bg-white/[0.06]"
      }`}
    >
      <div className="min-w-0 flex-1">
        <p className="line-clamp-2 whitespace-pre-wrap break-words leading-relaxed text-neutral-800 dark:text-neutral-100">
          {item.preview}
        </p>
        <p className="mt-1 text-[11px] text-neutral-400">
          {formatTime(item.last_copied_at)}
          {item.source_app ? ` · ${item.source_app}` : ""}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
        <button
          type="button"
          title={item.is_favorite ? "取消收藏" : "收藏"}
          aria-label={item.is_favorite ? "取消收藏" : "收藏"}
          onClick={(event) => {
            event.stopPropagation();
            onToggleFavorite();
          }}
          className="rounded-md p-1 text-base transition-colors hover:bg-black/10 dark:hover:bg-white/10"
        >
          {item.is_favorite ? "⭐️" : "☆"}
        </button>
        <button
          type="button"
          title="删除"
          aria-label="删除此条记录"
          onClick={(event) => {
            event.stopPropagation();
            onDelete();
          }}
          className="rounded-md p-1 text-base transition-colors hover:bg-red-500/10 hover:text-red-500"
        >
          🗑
        </button>
      </div>
      {item.is_favorite && (
        <span
          className="mt-1 shrink-0 text-sm text-amber-500 group-hover:hidden"
          aria-hidden="true"
        >
          ⭐️
        </span>
      )}
    </li>
  );
});
