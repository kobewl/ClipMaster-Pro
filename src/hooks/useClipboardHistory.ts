import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "@/lib/commands";
import {
  onClipboardCaptured,
  onClipboardCleared,
  onClipboardDeleted,
  onClipboardUpdated,
} from "@/lib/events";
import type { ClipboardItem, ListQuery } from "@/types/clipboard";
import { isCommandError, parseQueryTerms } from "@/types/clipboard";

/** 每页条数。滚动到底部时按这个粒度继续向数据库拉取。 */
const PAGE_SIZE = 100;
/** 与后端 list_clipboard_items 的 MAX_PAGE_SIZE 保持一致，单次查询不能超过它。 */
const MAX_PAGE_SIZE = 500;
/** 连续采集/更新时合并短时间内的事件，避免把 SQLite 查询队列塞满。 */
const EVENT_REFRESH_DELAY_MS = 120;

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
  /**
   * 续拉已经拉不动了（这一页 0 条新增）。
   *
   * 为什么需要它：后端重排窗口有 cap（500），命中超过 500 条时 `total` 仍报真实
   * COUNT，但第 501 条起永远取不到 —— 只看 `items.length < total` 的话
   * 「加载更多」按钮会永远显示、每次点都空跑一趟。这是**页面级**状态，
   * 查询条件一变就重置（新 query 是新的窗口，不能再沿用上一次的结论）。
   * `total` 口径不动：状态栏照旧说「共 N 条」。
   */
  const [loadMoreStopped, setLoadMoreStopped] = useState(false);
  /**
   * 生成当前列表那次请求的查询词总数（命中 chip 的分母）。
   *
   * 刻意**不从实时输入算**（在 App 里 `parseQueryTerms(当前输入词)` 也能算）：
   * 分母必须和分子 `matched_terms` 出自同一次响应，否则用户改词的瞬间会渲染出
   * 「命中 1/2 词」这种把旧响应挂在新查询上的假数字 —— 证据一旦张冠李戴，
   * 还不如不给。所以它跟 items 一样，只在响应落地时更新。
   */
  const [queryTermCount, setQueryTermCount] = useState(0);
  /**
   * 本次列表响应里放宽召回被筛掉的词；null = 没走放宽（严格路径或空 query）。
   * 与 items 同代：新查询发起时先清空，免得把上一次查询的"被筛除的词"挂过去。
   */
  const [relaxedDropped, setRelaxedDropped] = useState<string[] | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const generationRef = useRef(0);
  const loadingMoreRef = useRef(false);
  const loadedCountRef = useRef(0);
  const refreshTimerRef = useRef<number | null>(null);
  /** 最新已提交的列表快照（响应回调里用来算"这一页新增了几条"）。 */
  const itemsRef = useRef<ClipboardItem[]>([]);
  /** `loadMoreStopped` 的 ref 镜像：`loadMore` 是稳定引用，读不到最新的 state。 */
  const loadMoreStoppedRef = useRef(false);

  // 渲染期同步查询条件与已加载数量：effect 与事件回调都晚于本次渲染，写进来的必定是最新值。
  const queryRef = useRef({ groupId, search, contentType, timeRange });
  queryRef.current = { groupId, search, contentType, timeRange };
  loadedCountRef.current = items.length;
  itemsRef.current = items;
  loadMoreStoppedRef.current = loadMoreStopped;

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
      if (reset) {
        setLoadState("loading");
        // 新查询 = 新的候选窗口：上一次「拉不动了」的结论与放宽证据都不能沿用。
        setLoadMoreStopped(false);
        setRelaxedDropped(null);
      }

      const limit = reset
        ? PAGE_SIZE
        : Math.min(MAX_PAGE_SIZE, Math.max(PAGE_SIZE, loadedCountRef.current));
      // 本次请求实际用的查询串（与 buildQuery 的 trim 口径一致），
      // 响应落地时按它算 chip 分母。
      const requestTermCount = parseQueryTerms(queryRef.current.search).length;

      commands
        .listClipboardItems(buildQuery(limit, 0))
        .then((result) => {
          if (generation !== generationRef.current) return;
          setItems(result.items);
          setTotal(result.total);
          // 宽松路径的证据随本次响应更新；严格路径/空查询时是 undefined → null。
          setRelaxedDropped(result.relaxed_dropped ?? null);
          setQueryTermCount(requestTermCount);
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

  const scheduleReload = useCallback(() => {
    if (refreshTimerRef.current !== null) return;
    refreshTimerRef.current = window.setTimeout(() => {
      refreshTimerRef.current = null;
      fetchFirstPage(false);
    }, EVENT_REFRESH_DELAY_MS);
  }, [fetchFirstPage]);

  // 分组 / 关键词 / 筛选条件变化 → 回到第一页。
  useEffect(() => {
    fetchFirstPage(true);
  }, [groupId, search, contentType, timeRange, fetchFirstPage]);

  /** 续拉下一页，追加到列表尾部。 */
  const loadMore = useCallback(() => {
    if (loadingMoreRef.current) return;
    // 已经确认拉不动了（上一次续拉 0 条新增），别再空跑一趟数据库。
    if (loadMoreStoppedRef.current) return;
    const offset = loadedCountRef.current;
    if (offset === 0) return;
    const generation = generationRef.current;
    loadingMoreRef.current = true;
    setLoadingMore(true);

    commands
      .listClipboardItems(buildQuery(PAGE_SIZE, offset))
      .then((result) => {
        if (generation !== generationRef.current) return;
        // 「本页新增条数」用去重后的结果判断：后端在重排窗口（500）之外
        // 永远给不出新条目，这一页必然是 0 条新增 —— 就此停住。
        const merged = appendUnique(itemsRef.current, result.items);
        if (merged.length === itemsRef.current.length) setLoadMoreStopped(true);
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
        if (!disposed) scheduleReload();
      }),
      onClipboardUpdated(() => {
        if (!disposed) scheduleReload();
      }),
      onClipboardCleared(() => {
        if (!disposed) scheduleReload();
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
      if (refreshTimerRef.current !== null) {
        window.clearTimeout(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
      unsubscribers.forEach((pending) => pending.then((fn) => fn()).catch(() => {}));
    };
  }, [scheduleReload]);

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
    // 「还有下一页」= 总数没拉满，且没被判定为拉不动（重排窗口 cap 之外取不到）。
    hasMore: !loadMoreStopped && items.length < total,
    /** 生成当前列表那次请求的查询词总数（命中 chip 的分母）。 */
    queryTermCount,
    /** 本次查询走了放宽召回时被筛掉的词（null = 没走放宽，不画横幅）。 */
    relaxedDropped,
    errorMessage,
    clearError,
    reload,
    loadMore,
    setItemGroup,
    deleteItem,
    copyItem,
  };
}
