import { useCallback, useEffect, useState } from "react";
import { commands } from "@/lib/commands";
import { onGroupsChanged } from "@/lib/events";
import type { ClipGroup } from "@/types/clipboard";

export function useGroups() {
  const [groups, setGroups] = useState<ClipGroup[]>([]);

  const reload = useCallback(() => {
    commands.listGroups().then(setGroups).catch(() => {});
  }, []);

  useEffect(() => { reload(); }, [reload]);

  useEffect(() => {
    let disposed = false;
    const unlisten = onGroupsChanged(() => { if (!disposed) reload(); });
    return () => { disposed = true; unlisten.then((fn) => fn()).catch(() => {}); };
  }, [reload]);

  return { groups, reload };
}
