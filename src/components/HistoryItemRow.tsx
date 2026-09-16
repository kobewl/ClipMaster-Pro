import { memo, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ClipboardItem, ClipGroup } from "@/types/clipboard";
import { getAppIcon, extractDomain } from "@/lib/sourceIcons";

interface Props {
  item: ClipboardItem;
  active: boolean;
  groups: ClipGroup[];
  onClick: () => void;
  onSetGroup: (groupId: string | null) => void;
  onDelete: () => void;
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const diff = Date.now() - d.getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return "刚刚";
  if (m < 60) return `${m}分钟前`;
  const h = Math.floor(diff / 3600000);
  if (h < 24) return `${h}小时前`;
  const day = Math.floor(diff / 86400000);
  if (day < 7) return `${day}天前`;
  return d.toLocaleDateString(undefined, { month: "2-digit", day: "2-digit" });
}

export const HistoryItemRow = memo(function HistoryItemRow({
  item,
  active,
  groups,
  onClick,
  onSetGroup,
  onDelete,
}: Props) {
  const ref = useRef<HTMLLIElement>(null);
  const [groupMenuOpen, setGroupMenuOpen] = useState(false);

  useEffect(() => {
    if (active && ref.current) {
      ref.current.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }, [active]);

  const isImage = item.content_type === "image";
  const domain = extractDomain(item.source_url);
  const appIcon = getAppIcon(item.source_app);
  const group = item.group_id ? groups.find((g) => g.id === item.group_id) : null;

  return (
    <li
      ref={ref}
      role="option"
      aria-selected={active}
      onClick={onClick}
      className={`group relative flex cursor-pointer flex-col gap-1 rounded-lg px-3 py-2 text-sm transition-colors ${
        active
          ? "bg-blue-500/10 ring-1 ring-blue-500/15 dark:bg-blue-400/15 dark:ring-blue-400/15"
          : "hover:bg-black/[0.03] dark:hover:bg-white/[0.04]"
      }`}
    >
      {/* Header: source + time */}
      <div className="flex items-center justify-between text-[11px]">
        <span className="flex items-center gap-1 text-neutral-400">
          <span>{appIcon}</span>
          <span className="max-w-[140px] truncate">
            {domain ?? item.source_app ?? ""}
          </span>
        </span>
        <span className="text-neutral-400">{formatTime(item.last_copied_at)}</span>
      </div>

      {/* Content */}
      {isImage ? (
        <div className="flex items-center gap-2">
          <img
            src={convertFileSrc(item.content_text)}
            alt="截图"
            className="h-14 max-w-[140px] rounded border border-black/5 object-cover dark:border-white/10"
            loading="lazy"
          />
          <span className="text-[11px] text-neutral-400">📷 图片</span>
        </div>
      ) : (
        <p className="line-clamp-2 whitespace-pre-wrap break-words text-[13px] leading-relaxed text-neutral-800 dark:text-neutral-100">
          {item.preview}
        </p>
      )}

      {/* Footer: group badge */}
      {group && (
        <span className="flex w-fit items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px]"
              style={{ backgroundColor: group.color + "18", color: group.color }}>
          <span className="inline-block h-1.5 w-1.5 rounded-full" style={{ backgroundColor: group.color }} />
          {group.name}
        </span>
      )}

      {/* Hover actions */}
      <div className="absolute right-2 top-2 flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
        <div className="relative">
          <button
            type="button"
            title="设置分组"
            onClick={(e) => { e.stopPropagation(); setGroupMenuOpen(!groupMenuOpen); }}
            className="rounded-md p-1 text-[11px] transition-colors hover:bg-black/10 dark:hover:bg-white/10"
          >
            📁
          </button>
          {groupMenuOpen && (
            <div className="absolute right-0 top-7 z-30 min-w-[120px] rounded-lg bg-white py-1 shadow-xl ring-1 ring-black/10 dark:bg-neutral-700 dark:ring-white/10"
                 onClick={(e) => e.stopPropagation()}>
              {item.group_id && (
                <button type="button"
                        onClick={() => { onSetGroup(null); setGroupMenuOpen(false); }}
                        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[11px] text-neutral-500 hover:bg-black/[0.04] dark:hover:bg-white/[0.06]">
                  移出分组
                </button>
              )}
              {groups.map((g) => (
                <button key={g.id} type="button"
                        onClick={() => { onSetGroup(g.id); setGroupMenuOpen(false); }}
                        className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-[11px] hover:bg-black/[0.04] dark:hover:bg-white/[0.06] ${
                          item.group_id === g.id ? "text-blue-500 font-medium" : "text-neutral-700 dark:text-neutral-200"
                        }`}>
                  <span className="h-2 w-2 rounded-full" style={{ backgroundColor: g.color }} />
                  {g.name}
                </button>
              ))}
            </div>
          )}
        </div>
        <button
          type="button"
          title="删除"
          onClick={(e) => { e.stopPropagation(); onDelete(); }}
          className="rounded-md p-1 text-[11px] transition-colors hover:bg-red-500/10 hover:text-red-500"
        >
          🗑
        </button>
      </div>
    </li>
  );
});
