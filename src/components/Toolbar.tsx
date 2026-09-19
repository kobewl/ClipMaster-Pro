import { useEffect, useRef, useState } from "react";
import type { ClipGroup } from "@/types/clipboard";
import { GroupDialog } from "./GroupDialog";
import { Icon, type IconName } from "./Icon";

/** 内容类型 tab：value = 后端 content_type 字符串，null = 全部。 */
const CONTENT_TYPES: Array<{ value: string | null; label: string; icon: IconName }> = [
  { value: null, label: "全部类型", icon: "clipboard" },
  { value: "text", label: "文本", icon: "type" },
  { value: "image", label: "图片", icon: "image" },
  { value: "html", label: "富文本", icon: "code" },
  { value: "files", label: "文件", icon: "folder" },
];

/** 预设时间档（value = 后端 time_range 字符串，null = 全部时间）。 */
const TIME_RANGES: Array<{ value: string | null; label: string }> = [
  { value: null, label: "全部时间" },
  { value: "today", label: "今天" },
  { value: "week", label: "近 7 天" },
  { value: "month", label: "近 30 天" },
];

interface ToolbarProps {
  groups: ClipGroup[];
  activeGroupId: string | null;
  onSelect: (groupId: string | null) => void;
  onGroupsReload: () => void;
  contentType: string | null;
  timeRange: string | null;
  onContentTypeChange: (value: string | null) => void;
  onTimeRangeChange: (value: string | null) => void;
}

/**
 * 搜索框下方的唯一工具栏，一行承载三个维度（参照 Raycast / Pastebot 的布局）：
 *
 *   [类型图标 tabs] │ [分组 chips 横向滚动…]      [时间下拉] [＋管理]
 *
 * 类型是「系统属性」用图标 tab 固定在左；分组是「用户整理」用文字 chip
 * 占据弹性中段；时间是低频操作收进右侧图标下拉，选中后才展开显示标签。
 * 类型 tabs 与分组之间用竖分隔线划清「系统 / 自定义」的语义边界。
 */
export function Toolbar({
  groups,
  activeGroupId,
  onSelect,
  onGroupsReload,
  contentType,
  timeRange,
  onContentTypeChange,
  onTimeRangeChange,
}: ToolbarProps) {
  const [manageOpen, setManageOpen] = useState(false);

  return (
    <>
      <nav className="group-bar" aria-label="历史筛选与分组">
        <div
          className="flex flex-none items-center gap-0.5"
          role="group"
          aria-label="按类型筛选"
        >
          {CONTENT_TYPES.map(({ value, label, icon }) => {
            const active = contentType === value;
            return (
              <button
                key={label}
                type="button"
                title={label}
                aria-label={label}
                aria-pressed={active}
                onClick={() => onContentTypeChange(value)}
                className={`flex h-[26px] w-[28px] items-center justify-center rounded-[var(--cm-radius-sm)] transition-colors ${
                  active
                    ? "bg-[var(--cm-accent-soft)] text-[var(--cm-accent-text)]"
                    : "text-[var(--cm-fg-muted)] hover:bg-[var(--cm-hover)] hover:text-[var(--cm-fg)]"
                }`}
              >
                <Icon name={icon} className="h-[15px] w-[15px]" />
              </button>
            );
          })}
        </div>

        {/* 竖分隔线：左边系统属性，右边用户自定义分组 */}
        <div className="h-4 w-px flex-none bg-[var(--cm-line)]" aria-hidden="true" />

        <div className="scrollbar-thin group-bar__scroller">
          <TabButton
            active={activeGroupId === null}
            onClick={() => onSelect(null)}
            label="全部"
            title="显示所有记录"
          />
          {groups.map((group) => (
            <TabButton
              key={group.id}
              active={activeGroupId === group.id}
              onClick={() => onSelect(group.id)}
              label={group.name}
              color={group.color}
              count={group.item_count}
              title={`${group.name} · ${group.item_count} 条`}
            />
          ))}
        </div>

        <TimeMenu value={timeRange} onChange={onTimeRangeChange} />

        <button
          type="button"
          onClick={() => setManageOpen(true)}
          title="管理分组"
          aria-label="管理分组"
          className="group-add"
        >
          <Icon name="plus" />
          <span>管理</span>
        </button>
      </nav>

      {manageOpen && (
        <GroupDialog
          groups={groups}
          onClose={() => setManageOpen(false)}
          onChanged={onGroupsReload}
        />
      )}
    </>
  );
}

/** 时间筛选下拉：按钮只占一个图标位（选中非默认档时附带标签），点外关闭。 */
function TimeMenu({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (value: string | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const active = TIME_RANGES.find((r) => r.value === value) ?? TIME_RANGES[0];
  const isActive = value !== null;

  // 点菜单外面自动收起：挂一个全屏透明层拦截下一次点击，比监听 document 焦点更省事。
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  return (
    <div ref={rootRef} className="relative flex-none">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        title="按时间筛选"
        className={`flex h-[26px] items-center gap-1 rounded-[var(--cm-radius-sm)] px-1.5 text-[12px] font-medium transition-colors ${
          isActive
            ? "bg-[var(--cm-accent-soft)] text-[var(--cm-accent-text)]"
            : "text-[var(--cm-fg-muted)] hover:bg-[var(--cm-hover)] hover:text-[var(--cm-fg)]"
        }`}
      >
        <Icon name="clock" className="h-[15px] w-[15px]" />
        {isActive && <span>{active.label}</span>}
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} aria-hidden="true" />
          <div
            role="menu"
            className="absolute right-0 top-full z-50 mt-1.5 w-32 rounded-[10px] border border-[var(--cm-line)] bg-[var(--cm-surface)] py-1 shadow-[0_8px_24px_rgba(0,0,0,0.14)]"
          >
            {TIME_RANGES.map(({ value: v, label }) => (
              <button
                key={label}
                type="button"
                role="menuitemradio"
                aria-checked={value === v}
                onClick={() => {
                  onChange(v);
                  setOpen(false);
                }}
                className={`flex w-full items-center justify-between px-3 py-1.5 text-left text-[12px] transition-colors hover:bg-[var(--cm-hover)] ${
                  value === v ? "text-[var(--cm-accent-text)]" : "text-[var(--cm-fg)]"
                }`}
              >
                {label}
                {value === v && <Icon name="check" className="h-3.5 w-3.5" />}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function TabButton({
  active,
  onClick,
  label,
  color,
  count,
  title,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  color?: string;
  count?: number;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-pressed={active}
      className={`group-chip ${
        active
          ? "group-chip--active"
          : ""
      }`}
    >
      {color && (
        <span
          className="inline-block h-2 w-2 shrink-0 rounded-full"
          style={{ backgroundColor: color }}
        />
      )}
      <span className="max-w-[110px] truncate">{label}</span>
      {count !== undefined && count > 0 && (
        <span className="text-[10px] tabular-nums opacity-60">{count}</span>
      )}
    </button>
  );
}
