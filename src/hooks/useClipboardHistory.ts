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

interface UseClipboardHistoryOptions {
  favoritesOnly: boolean;
  search: string;
}

interface UseClipboardHistoryResult {
  items: ClipboardItem[];
  total: number;
  loadState: LoadState;
  errorMessage: string | null;
  reload: () => void;
  toggleFavorite: (id: string) => Promise<void>;
  deleteItem: (id: string) => Promise<void>;
  copyItem: (id: string) => Promise<void>;
}

/**
 * 历史列表数据源。
 * 使用 generation 计数器防止旧查询覆盖新查询结果
 * （架构文档 8 节并发规则、US-004 验收标准）。
 */
export function useClipboardHistory(
  options: UseClipboardHistoryOptions,
): UseClipboardHistoryResult {
  const { favoritesOnly, search } = options;
  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [total, setTotal] = useState(0);
  const [loadState, setLoadState] = useState<LoadState>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const generationRef = useRef(0);

  const load = useCallback(() => {
    const currentGeneration = ++generationRef.current;
    setLoadState("loading");
    setErrorMessage(null);

    commands
      .listClipboardItems({
        favorites_only: favoritesOnly,
        search: search.trim().length > 0 ? search.trim() : null,
        limit: PAGE_SIZE,
        offset: 0,
      })
      .then((result) => {
        if (currentGeneration !== generationRef.current) {
          // 已经有更新的查询发出，丢弃这次的结果。
          return;
        }
        setItems(result.items);
        setTotal(result.total);
        setLoadState("idle");
      })
      .catch((error: unknown) => {
        if (currentGeneration !== generationRef.current) {
          return;
        }
        setLoadState("error");
        setErrorMessage(
          isCommandError(error) ? error.message : "加载历史记录失败",
        );
      });
  }, [favoritesOnly, search]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    let disposed = false;
    const unlistenPromises = [
      onClipboardCaptured(() => {
        if (!disposed) {
          load();
        }
      }),
      onClipboardDeleted(() => {
        if (!disposed) {
          load();
        }
      }),
      onClipboardUpdated(() => {
        if (!disposed) {
          load();
        }
      }),
      onClipboardCleared(() => {
        if (!disposed) {
          load();
        }
      }),
    ];

    return () => {
      disposed = true;
      unlistenPromises.forEach((p) => {
        p.then((unlisten) => unlisten()).catch(() => {
          /* 监听已被清理，忽略 */
        });
      });
    };
  }, [load]);

  const toggleFavorite = useCallback(
    async (id: string) => {
      const target = items.find((item) => item.id === id);
      if (!target) return;
      const nextFavorite = !target.is_favorite;

      // 乐观更新，失败时回滚，符合 US-006 验收标准。
      setItems((prev) =>
        prev.map((item) =>
          item.id === id ? { ...item, is_favorite: nextFavorite } : item,
        ),
      );

      try {
        await commands.setFavorite(id, nextFavorite);
      } catch (error) {
        setItems((prev) =>
          prev.map((item) =>
            item.id === id
              ? { ...item, is_favorite: target.is_favorite }
              : item,
          ),
        );
        setErrorMessage(
          isCommandError(error) ? error.message : "更新收藏状态失败",
        );
      }
    },
    [items],
  );

  const deleteItem = useCallback(
    async (id: string) => {
      try {
        await commands.deleteClipboardItem(id);
        setItems((prev) => prev.filter((item) => item.id !== id));
        setTotal((prev) => Math.max(0, prev - 1));
      } catch (error) {
        setErrorMessage(
          isCommandError(error) ? error.message : "删除历史记录失败",
        );
      }
    },
    [],
  );

  const copyItem = useCallback(async (id: string) => {
    try {
      await commands.copyClipboardItem(id);
    } catch (error) {
      setErrorMessage(
        isCommandError(error) ? error.message : "写入剪贴板失败",
      );
      throw error;
    }
  }, []);

  return {
    items,
    total,
    loadState,
    errorMessage,
    reload: load,
    toggleFavorite,
    deleteItem,
    copyItem,
  };
}
