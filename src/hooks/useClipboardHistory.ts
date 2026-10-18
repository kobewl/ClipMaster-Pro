import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "@/lib/commands";
import {
  onClipboardCaptured,
  onClipboardCleared,
  onClipboardDeleted,
  onClipboardUpdated,
} from "@/lib/events";
import { isWindowHidden, onWindowShown } from "@/lib/windowVisibility";
import type { ClipboardItem, ListQuery, ListResult } from "@/types/clipboard";
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

/** 浅比较单个字段：数组（如 `matched_terms`）按长度与元素逐一比，其余用 Object.is。 */
function sameFieldValue(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((value, index) => Object.is(value, b[index]));
  }
  return false;
}

/** 两条记录逐字段浅比较（取两边 key 的并集，缺失的 key 视为 undefined）。 */
function sameItem(a: ClipboardItem, b: ClipboardItem): boolean {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
  for (const key of keys) {
    const field = key as keyof ClipboardItem;
    if (!sameFieldValue(a[field], b[field])) return false;
  }
  return true;
}

/**
 * 刷新结果与旧列表对齐：按 id 找旧对象，字段全同就复用旧引用；
 * 如果结果与 prev 长度相同且每个位置都是同一个引用，直接返回 prev 本身。
 *
 * 为什么：后台刷新每次都会拿到一份全新的对象数组，直接替换会让 HistoryItemRow 的
 * memo 全部失效（每行都重渲），HistoryList 里依赖 items 的 prefetchSourceIcons
 * effect 也会重跑。复用引用后，只有真正变了的行才重渲；什么都没变时 setItems
 * 拿到的是同一个数组，React 直接跳过这次更新。
 */
function reconcileItems(prev: ClipboardItem[], next: ClipboardItem[]): ClipboardItem[] {
  const known = new Map(prev.map((item) => [item.id, item]));
  let unchanged = prev.length === next.length;
  const merged = next.map((item, index) => {
    const old = known.get(item.id);
    const reused = old !== undefined && sameItem(old, item) ? old : item;
    if (reused !== prev[index]) unchanged = false;
    return reused;
  });
  return unchanged ? prev : merged;
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
 *    窗口不可见时 captured / updated 只拉第一页并拼到列表头部（见 fetchFirstPage 的
 *    headOnly），同时记 dirty，重新显示 / 获得焦点时再补一次完整窗口的刷新；
 * 4. 事件触发的刷新（`reload`）保留"已加载这么多条"的窗口，只有查询条件变化
 *    才真正回到第一页 —— 否则用户刚往下滚动，一次系统复制就把他弹回顶部。
 *    已加载超过 500 条（后端单次上限）时按 500 一批顺序拉，不截断；
 *    结果按 id 与旧列表对齐、复用没变的行对象（见 reconcileItems）。
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
  /** 窗口不可见期间收到过 captured / updated / cleared，重新显示时要补一次完整窗口的刷新。 */
  const dirtyRef = useRef(false);
  /** 防抖窗口里有没有「要完整窗口」的请求；全是隐藏期间的请求时只拉第一页。 */
  const pendingFullRefreshRef = useRef(false);

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
   * @param headOnly 只在 `reset=false` 时有意义：只拉第一页（PAGE_SIZE 条），结果放到
   *   列表头部，旧列表里不在这一页的条目原样接在后面，总长度不超过
   *   max(旧长度, PAGE_SIZE)（窗口大小不变）。窗口隐藏时用：用户呼出窗口后
   *   最常见的操作是直接回车粘贴第一条，第一条必须已经是刚复制的内容，不能等显示后
   *   再刷新（那样回车可能把旧的第一条粘到别的应用里）。完整窗口的对齐留给显示后
   *   那一次刷新，隐藏期间连续复制也只是每次一页的开销。
   */
  const fetchFirstPage = useCallback(
    (reset: boolean, headOnly = false) => {
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

      // 新查询只拉第一页；后台刷新要拉回"已加载这么多条"。后端单次最多给
      // MAX_PAGE_SIZE 条，超过时按批顺序拉（offset 0、500、1000…），否则刷新一次
      // 列表就被截回 500 条，用户滚动位置也会被夹回去。
      const target =
        reset || headOnly ? PAGE_SIZE : Math.max(PAGE_SIZE, loadedCountRef.current);
      // 本次请求实际用的查询串（与 buildQuery 的 trim 口径一致），
      // 响应落地时按它算 chip 分母。
      const requestTermCount = parseQueryTerms(queryRef.current.search).length;

      const run = async () => {
        let collected: ClipboardItem[] = [];
        let last: ListResult | null = null;
        let offset = 0;
        while (offset < target) {
          const limit = Math.min(MAX_PAGE_SIZE, target - offset);
          const batch = await commands.listClipboardItems(buildQuery(limit, offset));
          // 每批都检查代数：中途换了查询或又触发了刷新，后面的批次不必再拉。
          if (generation !== generationRef.current) return;
          last = batch;
          // 批次边界上可能因为新采集而重复一条，按 id 去重。
          collected = appendUnique(collected, batch.items);
          if (batch.items.length < limit) break;
          offset += limit;
        }
        if (last === null) return;
        const result = last;
        // headOnly：第一页在前，旧列表里不在第一页的接在后面（被顶上来的条目
        // 已经在第一页里，appendUnique 不会再留一份在原位置）。合并后截回
        // max(旧长度, PAGE_SIZE)：被新条目挤出窗口的尾部条目丢掉，否则隐藏期间
        // 每采集一条，已加载窗口就永久 +1，之后每次刷新要拉回、渲染的行越来越多。
        setItems((prev) =>
          reconcileItems(
            prev,
            headOnly
              ? appendUnique(collected, prev).slice(0, Math.max(prev.length, PAGE_SIZE))
              : collected,
          ),
        );
        setTotal(result.total);
        // 宽松路径的证据随本次响应更新；严格路径/空查询时是 undefined → null。
        setRelaxedDropped(result.relaxed_dropped ?? null);
        setQueryTermCount(requestTermCount);
        setLoadState("idle");
      };

      run()
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

  /**
   * 防抖刷新。同一个防抖窗口里只要有一次请求要完整窗口，就按完整窗口刷；
   * 全部来自隐藏期间时只拉第一页（headOnly）。
   */
  const scheduleReload = useCallback(
    (headOnly: boolean) => {
      if (!headOnly) pendingFullRefreshRef.current = true;
      if (refreshTimerRef.current !== null) return;
      refreshTimerRef.current = window.setTimeout(() => {
        refreshTimerRef.current = null;
        const full = pendingFullRefreshRef.current;
        pendingFullRefreshRef.current = false;
        fetchFirstPage(false, !full);
      }, EVENT_REFRESH_DELAY_MS);
    },
    [fetchFirstPage],
  );

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
    // 窗口不可见时省掉完整窗口的刷新（用户看不到，回来时由 onWindowShown 补一次），
    // 但 captured / updated 仍要把第一页刷好：呼出后直接回车粘贴的是 items[0]，
    // 它必须是刚复制 / 刚粘贴过的那条，不能和显示后的刷新赛跑。
    const requestRefresh = (headWhileHidden: boolean) => {
      void isWindowHidden().then((hidden) => {
        if (disposed) return;
        if (!hidden) {
          scheduleReload(false);
          return;
        }
        dirtyRef.current = true;
        if (headWhileHidden) scheduleReload(true);
      });
    };
    const unsubscribers = [
      onClipboardCaptured(() => {
        if (!disposed) requestRefresh(true);
      }),
      onClipboardUpdated(() => {
        if (!disposed) requestRefresh(true);
      }),
      // 清空会删掉列表中段的条目，只拉第一页拼不出正确结果（旧条目会留在尾部），
      // 隐藏时只记 dirty。清空只由窗口里的按钮触发，隐藏时基本收不到。
      onClipboardCleared(() => {
        if (!disposed) requestRefresh(false);
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

  // 重新显示 / 获得焦点：隐藏期间错过了变更就立即补一次完整窗口的刷新（不等防抖）。
  // 两个触发源可能对同一次显示都触发，dirty 已经清掉，第二次是空操作。
  useEffect(
    () =>
      onWindowShown(() => {
        if (!dirtyRef.current) return;
        dirtyRef.current = false;
        // 还没发出的防抖刷新（隐藏时排的第一页）被这次完整刷新覆盖：留着它的话，
        // 它晚 120ms 发起、代数更新，会把这次完整刷新的结果丢掉。
        if (refreshTimerRef.current !== null) {
          window.clearTimeout(refreshTimerRef.current);
          refreshTimerRef.current = null;
        }
        pendingFullRefreshRef.current = false;
        fetchFirstPage(false);
      }),
    [fetchFirstPage],
  );

  const clearError = useCallback(() => setErrorMessage(null), []);

  const setItemGroup = useCallback(
    async (id: string, nextGroupId: string | null) => {
      // 先乐观更新，界面立刻有反馈；后端随后广播只带 id 的 clipboard://updated，
      // 前端据此回查列表完成重排（事件本身不带新数据）。
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
