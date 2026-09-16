import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "@/lib/commands";
import {
  onClipboardCaptured,
  onClipboardCleared,
  onClipboardDeleted,
  onClipboardUpdated,
} from "@/lib/events";
import type { ClipboardItem } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";

const PAGE_SIZE = 200;

export type LoadState = "idle" | "loading" | "error";

interface Options {
  groupId: string | null;
  search: string;
}

export function useClipboardHistory(options: Options) {
  const { groupId, search } = options;
  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [total, setTotal] = useState(0);
  const [loadState, setLoadState] = useState<LoadState>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const generationRef = useRef(0);

  const load = useCallback(() => {
    const gen = ++generationRef.current;
    setLoadState("loading");
    setErrorMessage(null);
    commands
      .listClipboardItems({
        group_id: groupId,
        search: search.trim().length > 0 ? search.trim() : null,
        limit: PAGE_SIZE,
        offset: 0,
      })
      .then((result) => {
        if (gen !== generationRef.current) return;
        setItems(result.items);
        setTotal(result.total);
        setLoadState("idle");
      })
      .catch((error: unknown) => {
        if (gen !== generationRef.current) return;
        setLoadState("error");
        setErrorMessage(isCommandError(error) ? error.message : "加载历史记录失败");
      });
  }, [groupId, search]);

  useEffect(() => { load(); }, [load]);

  useEffect(() => {
    let disposed = false;
    const unsubs = [
      onClipboardCaptured(() => { if (!disposed) load(); }),
      onClipboardDeleted(() => { if (!disposed) load(); }),
      onClipboardUpdated(() => { if (!disposed) load(); }),
      onClipboardCleared(() => { if (!disposed) load(); }),
    ];
    return () => {
      disposed = true;
      unsubs.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, [load]);

  const setItemGroup = useCallback(async (id: string, gid: string | null) => {
    setItems((prev) => prev.map((i) => (i.id === id ? { ...i, group_id: gid } : i)));
    try {
      await commands.setItemGroup(id, gid);
    } catch (error) {
      load();
      setErrorMessage(isCommandError(error) ? error.message : "设置分组失败");
    }
  }, [load]);

  const deleteItem = useCallback(async (id: string) => {
    try {
      await commands.deleteClipboardItem(id);
      setItems((prev) => prev.filter((i) => i.id !== id));
      setTotal((prev) => Math.max(0, prev - 1));
    } catch (error) {
      setErrorMessage(isCommandError(error) ? error.message : "删除失败");
    }
  }, []);

  const copyItem = useCallback(async (id: string) => {
    try {
      await commands.copyClipboardItem(id);
    } catch (error) {
      setErrorMessage(isCommandError(error) ? error.message : "写入剪贴板失败");
      throw error;
    }
  }, []);

  return { items, total, loadState, errorMessage, reload: load, setItemGroup, deleteItem, copyItem };
}
