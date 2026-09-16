import { useEffect, useState } from "react";

/**
 * 对搜索输入做防抖，满足 FR-SEA-002：输入搜索词后及时更新结果，并具备防抖。
 */
export function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);

  return debounced;
}
