import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "@/lib/commands";
import {
  onClipboardCaptured,
  onClipboardCleared,
  onClipboardDeleted,
  onClipboardUpdated,
} from "@/lib/events";
import type { ClipboardItem, ListQuery } from "@/types/clipboard";
import { isCommandError } from "@/types/clipboard";

/** 每页条数。滚动到底部时按这个粒度继续向数据库拉取。 */
const PAGE_SIZE = 100;
/** 与后端 list_clipboard_items 的 MAX_PAGE_SIZE 保持一致，单次查询不能超过它。 */
const MAX_PAGE_SIZE = 500;

export type LoadState = "idle" | "loading" | "error";

interface Options {
  groupId: string | null;
  search: string;
  /** 按内容类型筛选："text" | "image" | "html" | "files"。null = 全部。 */
  contentType: string | null;
  /** 预设时间档："today" | "week" | "month"。null = 全部时间。 */
  timeRange: string | null;
}

/** 把未知异常翻译成能给用户看的中文提示。 */
function toMessage(error: unknown, fallback: string): string {
  return isCommandError(error) ? error.message : fallback;
}

/** 追加分页结果时按 id 去重，并保留已存在条目的对象引用（让 memo 的行组件不重复渲染）。 */
function appendUnique(prev: ClipboardItem[], incoming: ClipboardItem[]): ClipboardItem[] {
  const known = new Set(prev.map((item) => item.id));
  const fresh = incoming.filter((item) => !known.has(item.id));
  return fresh.length === 0 ? prev : [...prev, ...fresh];
}

/**
 * 剪贴板历史列表数据源。
 *
 * 几个关键设计：
 * 1. 查询条件（分组 / 关键词）存在 ref 里，`reload` / `loadMore` 保持稳定引用，
 *    事件监听只挂载一次，翻页也不会因为依赖变化被打断；
 * 2. 每次重置查询都会让 generation（代数）自增，过期响应直接丢弃 —— 快速输入时
 *    先发的慢请求不会覆盖后发的快请求；
 * 3. `clipboard://deleted` 走本地摘除（后端已经删过了），其余事件才回查数据库；
 * 4. 事件触发的刷新（`reload`）保留"已加载这么多条"的窗口，只有查询条件变化
 *    才真正回到第一页 —— 否则用户刚往下滚动，一次系统复制就把他弹回顶部。
 */
export function useClipboardHistory(options: Options) {
  const { groupId, search, contentType, timeRange } = options;

  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [total, setTotal] = useState(0);
  const [loadState, setLoadState] = useState<LoadState>("idle");
  const [loadingMore, setLoadingMore] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const generationRef = useRef(0);
  const loadingMoreRef = useRef(false);
  const loadedCountRef = useRef(0);

  // 渲染期同步查询条件与已加载数量：effect 与事件回调都晚于本次渲染，写进来的必定是最新值。
  const queryRef = useRef({ groupId, search, contentType, timeRange });
  queryRef.current = { groupId, search, contentType, timeRange };
  loadedCountRef.current = items.length;

  const buildQuery = useCallback((limit: number, offset: number): ListQuery => {
    const { groupId: currentGroup, search: keyword, contentType: type, timeRange: range } = queryRef.current;
    const trimmed = keyword.trim();
    return {
      group_id: currentGroup,
      search: trimmed.length > 0 ? trimmed : null,
      content_type: type,
      time_range: range,
      limit,
      offset,
    };
  }, []);

  /**
   * 重新查询。
   * @param reset 为 true 时回到第一页（查询条件变了）；为 false 时保留已加载窗口（后台数据变了）。
   */
  const fetchFirstPage = useCallback(
    (reset: boolean) => {
      const generation = ++generationRef.current;
      loadingMoreRef.current = false;
      setLoadingMore(false);
      setErrorMessage(null);
      if (reset) setLoadState("loading");

      const limit = reset
        ? PAGE_SIZE
        : Math.min(MAX_PAGE_SIZE, Math.max(PAGE_SIZE, loadedCountRef.current));

      commands
        .listClipboardItems(buildQuery(limit, 0))
        .then((result) => {
          if (generation !== generationRef.current) return;
          setItems(result.items);
          setTotal(result.total);
          setLoadState("idle");
        })
        .catch((error: unknown) => {
          if (generation !== generationRef.current) return;
          // 后台刷新失败时保留旧数据，只提示错误，不要把列表清空。
          setLoadState((prev) => (prev === "loading" ? "error" : prev));
          setErrorMessage(toMessage(error, "加载历史记录失败"));
        });
    },
    [buildQuery],
  );

  const reload = useCallback(() => fetchFirstPage(false), [fetchFirstPage]);

  // 分组 / 关键词 / 筛选条件变化 → 回到第一页。
  useEffect(() => {
    fetchFirstPage(true);
  }, [groupId, search, contentType, timeRange, fetchFirstPage]);

  /** 续拉下一页，追加到列表尾部。 */
  const loadMore = useCallback(() => {
    if (loadingMoreRef.current) return;
    const offset = loadedCountRef.current;
    if (offset === 0) return;
    const generation = generationRef.current;
    loadingMoreRef.current = true;
    setLoadingMore(true);

    commands
      .listClipboardItems(buildQuery(PAGE_SIZE, offset))
      .then((result) => {
        if (generation !== generationRef.current) return;
        setItems((prev) => appendUnique(prev, result.items));
        setTotal(result.total);
      })
      .catch((error: unknown) => {
        if (generation !== generationRef.current) return;
        setErrorMessage(toMessage(error, "加载更多失败"));
      })
      .finally(() => {
        if (generation !== generationRef.current) return;
        loadingMoreRef.current = false;
        setLoadingMore(false);
      });
  }, [buildQuery]);

  useEffect(() => {
    let disposed = false;
    const unsubscribers = [
      onClipboardCaptured(() => {
        if (!disposed) fetchFirstPage(false);
      }),
      onClipboardUpdated(() => {
        if (!disposed) fetchFirstPage(false);
      }),
      onClipboardCleared(() => {
        if (!disposed) fetchFirstPage(false);
      }),
      onClipboardDeleted((id) => {
        if (disposed) return;
        // 后端已完成删除，本地直接摘除，省掉一次全量查询。
        setItems((prev) => prev.filter((item) => item.id !== id));
        setTotal((prev) => Math.max(0, prev - 1));
      }),
    ];
    return () => {
      disposed = true;
      unsubscribers.forEach((pending) => pending.then((fn) => fn()).catch(() => {}));
    };
  }, [fetchFirstPage]);

  const clearError = useCallback(() => setErrorMessage(null), []);

  const setItemGroup = useCallback(
    async (id: string, nextGroupId: string | null) => {
      // 先乐观更新，界面立刻有反馈；后端随后广播 clipboard://updated 触发重排。
      setItems((prev) =>
        prev.map((item) => (item.id === id ? { ...item, group_id: nextGroupId } : item)),
      );
      try {
        await commands.setItemGroup(id, nextGroupId);
      } catch (error) {
        reload();
        setErrorMessage(toMessage(error, "设置分组失败"));
      }
    },
    [reload],
  );

  const deleteItem = useCallback(async (id: string) => {
    try {
      await commands.deleteClipboardItem(id);
      setItems((prev) => prev.filter((item) => item.id !== id));
      setTotal((prev) => Math.max(0, prev - 1));
    } catch (error) {
      setErrorMessage(toMessage(error, "删除失败"));
    }
  }, []);

  const copyItem = useCallback(async (id: string) => {
    try {
      await commands.copyClipboardItem(id);
    } catch (error) {
      setErrorMessage(toMessage(error, "写入剪贴板失败"));
      throw error;
    }
  }, []);

  return {
    items,
    total,
    loadState,
    loadingMore,
    hasMore: items.length < total,
    errorMessage,
    clearError,
    reload,
    loadMore,
    setItemGroup,
    deleteItem,
    copyItem,
  };
}
