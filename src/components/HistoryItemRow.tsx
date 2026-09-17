import { memo, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ClipboardItem, ClipGroup } from "@/types/clipboard";
import { getAppIcon, extractDomain } from "@/lib/sourceIcons";
import { HighlightedText } from "./HighlightedText";
import { Icon } from "./Icon";

interface Props {
  item: ClipboardItem;
  active: boolean;
  groups: ClipGroup[];
  keyword: string;
  /** 来源应用的真实图标路径（null 时退回 emoji / 图片图标）。 */
  iconSrc: string | null;
  onPaste: (id: string) => void;
  onSetGroup: (id: string, groupId: string | null) => void;
  onDelete: (id: string) => void;
}

/** 菜单大致高度，用来判断往下弹出会不会被列表底部裁掉。 */
const MENU_ESTIMATED_HEIGHT = 210;

/**
 * 关闭菜单的那一次点击不应该顺带触发"点击即粘贴"（与 macOS 原生菜单一致）。
 * 用模块级时间戳而不是组件状态，是为了让"点到别的行"也被吞掉。
 */
let suppressPasteUntil = 0;
function suppressNextPaste() {
  suppressPasteUntil = Date.now() + 250;
}

function formatTime(iso: string, now: number): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const minutes = Math.floor((now - date.getTime()) / 60000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes}分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}小时前`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}天前`;
  return date.toLocaleDateString(undefined, { month: "2-digit", day: "2-digit" });
}

export const HistoryItemRow = memo(function HistoryItemRow({
  item,
  active,
  groups,
  keyword,
  iconSrc,
  onPaste,
  onSetGroup,
  onDelete,
}: Props) {
  const rowRef = useRef<HTMLLIElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuUp, setMenuUp] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [imageBroken, setImageBroken] = useState(false);
  // 记「加载失败的那张图标」而不是一个布尔值：iconSrc 变了就自动重试。
  const [brokenIconSrc, setBrokenIconSrc] = useState<string | null>(null);

  // 相对时间（"3分钟前"）需要自己走时，否则列表放着不动文案就会过期。
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (active) rowRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [active]);

  // 点击菜单以外任何地方、或按 Esc，都关掉菜单。
  useEffect(() => {
    if (!menuOpen) return;

    function handleDocumentClick(event: MouseEvent) {
      if (menuRef.current?.contains(event.target as Node)) return;
      suppressNextPaste();
      setMenuOpen(false);
    }
    function handleEscape(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      event.stopPropagation();
      setMenuOpen(false);
    }

    document.addEventListener("click", handleDocumentClick, true);
    window.addEventListener("keydown", handleEscape, true);
    return () => {
      document.removeEventListener("click", handleDocumentClick, true);
      window.removeEventListener("keydown", handleEscape, true);
    };
  }, [menuOpen]);

  function toggleMenu() {
    if (!menuOpen) {
      const rect = rowRef.current?.getBoundingClientRect();
      if (rect) setMenuUp(window.innerHeight - rect.bottom < MENU_ESTIMATED_HEIGHT);
    }
    setMenuOpen((open) => !open);
  }

  function closeMenu() {
    suppressNextPaste();
    setMenuOpen(false);
  }

  function handleRowClick() {
    if (Date.now() < suppressPasteUntil) return;
    onPaste(item.id);
  }

  const isImage = item.content_type === "image";
  const domain = extractDomain(item.source_url);
  const appIcon = getAppIcon(item.source_app);
  const group = item.group_id ? groups.find((g) => g.id === item.group_id) : null;

  // 真实的来源应用图标优先；取不到时才退回「图片类型 → 图片图标 / 文本类型 → emoji」。
  const iconUrl = iconSrc && iconSrc !== brokenIconSrc ? convertFileSrc(iconSrc) : null;

  return (
    <li
      ref={rowRef}
      role="option"
      aria-selected={active}
      onClick={handleRowClick}
      className={`history-card group ${
        active
          ? "history-card--active"
          : ""
      }`}
    >
      <div className="source-tile" aria-hidden>
        {iconUrl ? (
          <img src={iconUrl} alt="" loading="lazy" onError={() => setBrokenIconSrc(iconSrc)} />
        ) : isImage ? (
          <Icon name="image" />
        ) : (
          <span>{appIcon}</span>
        )}
      </div>
      <div className="history-card__body">
        <div className="history-card__meta">
          <span className="truncate">{domain ?? item.source_app ?? "未知来源"}</span>
          <span className="meta-dot" />
          <time>{formatTime(item.last_copied_at, now)}</time>
          {group && (
            <span className="group-badge" style={{ backgroundColor: `${group.color}16`, color: group.color }}>
              <i style={{ backgroundColor: group.color }} />{group.name}
            </span>
          )}
        </div>
        {isImage ? (
          <div className="image-preview">
            {imageBroken ? (
              <span className="image-preview__fallback"><Icon name="image" /></span>
            ) : (
              <img src={convertFileSrc(item.content_text)} alt="剪贴板图片" loading="lazy" onError={() => setImageBroken(true)} />
            )}
            <div><strong>图片</strong><span>点击即可粘贴原图</span></div>
          </div>
        ) : (
          <p className="history-card__text">
            <HighlightedText text={item.preview} keyword={keyword} />
          </p>
        )}
      </div>
      {active && <kbd className="paste-hint">↵ 粘贴</kbd>}

      {/* 悬停时出现的操作区。未悬停时不可点击，避免误触看不见的按钮。
          菜单打开时必须强制可见：容器上的 opacity 会一并作用到菜单子树。
          focus-within 让键盘 Tab 进来时按钮同样可见。 */}
      <div
        className={`history-actions ${
          menuOpen
            ? "pointer-events-auto opacity-100"
            : "pointer-events-none opacity-0 group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100"
        }`}
      >
        <div ref={menuRef} className="relative">
          <button
            type="button"
            title="设置分组"
            aria-label="设置分组"
            aria-expanded={menuOpen}
            onClick={(event) => {
              event.stopPropagation();
              toggleMenu();
            }}
            className={`card-action ${menuOpen ? "card-action--active" : ""}`}
          >
            <Icon name="folder" />
          </button>

          {menuOpen && (
            <div
              role="menu"
              onClick={(event) => event.stopPropagation()}
              className={`card-menu scrollbar-thin ${
                menuUp ? "bottom-7" : "top-7"
              }`}
            >
              {item.group_id && (
                <button
                  type="button"
                  onClick={() => {
                    onSetGroup(item.id, null);
                    closeMenu();
                  }}
                  className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[11px] text-neutral-500 hover:bg-black/[0.04] dark:hover:bg-white/[0.06]"
                >
                  移出分组
                </button>
              )}
              {groups.length === 0 && (
                <p className="px-3 py-1.5 text-[11px] text-neutral-400">
                  还没有分组，先在顶部「＋」里创建
                </p>
              )}
              {groups.map((g) => (
                <button
                  key={g.id}
                  type="button"
                  onClick={() => {
                    onSetGroup(item.id, g.id);
                    closeMenu();
                  }}
                  className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-[11px] hover:bg-black/[0.04] dark:hover:bg-white/[0.06] ${
                    item.group_id === g.id
                      ? "font-medium text-blue-500"
                      : "text-neutral-700 dark:text-neutral-200"
                  }`}
                >
                  <span className="h-2 w-2 shrink-0 rounded-full" style={{ backgroundColor: g.color }} />
                  <span className="truncate">{g.name}</span>
                </button>
              ))}
            </div>
          )}
        </div>

        <button
          type="button"
          title="删除"
          aria-label="删除这条记录"
          onClick={(event) => {
            event.stopPropagation();
            onDelete(item.id);
          }}
          className="card-action card-action--danger"
        >
          <Icon name="trash" />
        </button>
      </div>
    </li>
  );
});
