import { useEffect, useRef } from "react";

interface SearchBarProps {
  value: string;
  onChange: (value: string) => void;
  favoritesOnly: boolean;
  onFavoritesOnlyChange: (value: boolean) => void;
  autoFocus?: boolean;
}

export function SearchBar({
  value,
  onChange,
  favoritesOnly,
  onFavoritesOnlyChange,
  autoFocus,
}: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (autoFocus) {
      inputRef.current?.focus();
    }
  }, [autoFocus]);

  return (
    <div
      className="flex items-center gap-2 border-b border-black/[0.06] px-3 pb-2 pt-2 dark:border-white/[0.08]"
      data-tauri-drag-region
    >
      <div className="relative flex-1">
        <svg
          className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-neutral-400"
          viewBox="0 0 20 20"
          fill="currentColor"
        >
          <path
            fillRule="evenodd"
            d="M9 3.5a5.5 5.5 0 100 11 5.5 5.5 0 000-11zM2 9a7 7 0 1112.452 4.391l3.328 3.329a.75.75 0 11-1.06 1.06l-3.329-3.328A7 7 0 012 9z"
            clipRule="evenodd"
          />
        </svg>
        <input
          ref={inputRef}
          type="text"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          placeholder="搜索历史记录…"
          className="w-full rounded-lg bg-black/[0.04] py-1.5 pl-8 pr-7 text-sm outline-none placeholder:text-neutral-400 focus:bg-black/[0.07] focus:ring-1 focus:ring-blue-500/30 dark:bg-white/[0.08] dark:focus:bg-white/[0.12]"
          aria-label="搜索剪贴板历史"
        />
        {value.length > 0 && (
          <button
            type="button"
            onClick={() => {
              onChange("");
              inputRef.current?.focus();
            }}
            className="absolute right-2 top-1/2 -translate-y-1/2 rounded-full p-0.5 text-neutral-400 transition-colors hover:bg-black/10 hover:text-neutral-600 dark:hover:bg-white/10 dark:hover:text-neutral-200"
            aria-label="清空搜索条件"
            title="清空搜索条件"
          >
            <svg className="h-3 w-3" viewBox="0 0 12 12" fill="currentColor">
              <path d="M3.05 3.05a.75.75 0 011.06 0L6 4.94l1.89-1.89a.75.75 0 111.06 1.06L7.06 6l1.89 1.89a.75.75 0 11-1.06 1.06L6 7.06 4.11 8.95a.75.75 0 11-1.06-1.06L4.94 6 3.05 4.11a.75.75 0 010-1.06z" />
            </svg>
          </button>
        )}
      </div>
      <button
        type="button"
        onClick={() => onFavoritesOnlyChange(!favoritesOnly)}
        aria-pressed={favoritesOnly}
        title="只看收藏"
        className={`flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-medium transition-colors ${
          favoritesOnly
            ? "bg-amber-400/20 text-amber-600 ring-1 ring-amber-400/30 dark:text-amber-400"
            : "bg-black/[0.04] text-neutral-500 hover:bg-black/[0.07] dark:bg-white/[0.08] dark:text-neutral-400 dark:hover:bg-white/[0.12]"
        }`}
      >
        ⭐️ 收藏
      </button>
    </div>
  );
}
