import { useState } from "react";
import type { ClipGroup } from "@/types/clipboard";
import { GroupDialog } from "./GroupDialog";
import { Icon } from "./Icon";

interface GroupBarProps {
  groups: ClipGroup[];
  activeGroupId: string | null;
  onSelect: (groupId: string | null) => void;
  onGroupsReload: () => void;
}

export function GroupBar({
  groups,
  activeGroupId,
  onSelect,
  onGroupsReload,
}: GroupBarProps) {
  const [manageOpen, setManageOpen] = useState(false);

  return (
    <>
      <nav className="group-bar" aria-label="剪贴板分组">
        <span className="group-bar__label">视图</span>
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
        </div>
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
