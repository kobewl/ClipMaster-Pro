import { useState } from "react";
import type { ClipGroup } from "@/types/clipboard";
import { GroupDialog } from "./GroupDialog";

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
      <div className="scrollbar-thin flex items-center gap-1 overflow-x-auto border-b border-black/[0.05] px-2 py-1.5 dark:border-white/[0.06]">
        <TabButton
          active={activeGroupId === null}
          onClick={() => onSelect(null)}
          label="全部"
        />
        {groups.map((g) => (
          <TabButton
            key={g.id}
            active={activeGroupId === g.id}
            onClick={() => onSelect(g.id)}
            label={g.name}
            color={g.color}
            count={g.item_count}
          />
        ))}
        <button
          type="button"
          onClick={() => setManageOpen(true)}
          className="ml-0.5 shrink-0 rounded-md px-1.5 py-1 text-[11px] text-neutral-400 transition-colors hover:bg-black/[0.05] hover:text-neutral-600 dark:hover:bg-white/[0.08] dark:hover:text-neutral-300"
          title="管理分组"
        >
          ＋
        </button>
      </div>

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
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  color?: string;
  count?: number;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium transition-all ${
        active
          ? "bg-blue-500/15 text-blue-600 ring-1 ring-blue-500/20 dark:bg-blue-400/20 dark:text-blue-300 dark:ring-blue-400/20"
          : "text-neutral-500 hover:bg-black/[0.04] hover:text-neutral-700 dark:text-neutral-400 dark:hover:bg-white/[0.06] dark:hover:text-neutral-200"
      }`}
    >
      {color && (
        <span
          className="inline-block h-2 w-2 rounded-full"
          style={{ backgroundColor: color }}
        />
      )}
      {label}
      {count !== undefined && count > 0 && (
        <span className="text-[10px] opacity-60">{count}</span>
      )}
    </button>
  );
}
