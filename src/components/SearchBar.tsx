import { useEffect, useRef } from "react";
import { BrandMark } from "./BrandMark";
import { Icon } from "./Icon";

/**
 * 搜索框的 id。
 *
 * 全局键盘处理（`HistoryList`）靠它区分「焦点在搜索框里」和「焦点在别处的输入控件里」：
 * 前者按回车要粘贴选中项，后者要留给控件自己。
 */
export const SEARCH_INPUT_ID = "cm-search-input";

interface SearchBarProps {
  value: string;
  onChange: (value: string) => void;
  onOpenSettings: () => void;
  autoFocus?: boolean;
}

export function SearchBar({
  value,
  onChange,
  onOpenSettings,
  autoFocus,
}: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (autoFocus) inputRef.current?.focus();
  }, [autoFocus]);

  // 窗口被全局快捷键唤起后，焦点自动回到搜索框，省去一次点击。
  // 只有当焦点不在任何输入控件上时才抢，避免打断设置面板里的输入。
  useEffect(() => {
    function handleWindowFocus() {
      const active = document.activeElement;
      const isTypingElsewhere =
        active instanceof HTMLElement &&
        active !== document.body &&
        active !== inputRef.current &&
        (active.tagName === "INPUT" ||
          active.tagName === "TEXTAREA" ||
          active.isContentEditable);
      if (isTypingElsewhere) return;
      inputRef.current?.focus();
    }

    window.addEventListener("focus", handleWindowFocus);
    return () => window.removeEventListener("focus", handleWindowFocus);
  }, []);

  // ⌘K / Ctrl+K（以及传统的 ⌘F）聚焦并全选搜索框内容。
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (!(event.metaKey || event.ctrlKey)) return;
      if (!["f", "k"].includes(event.key.toLowerCase())) return;
      event.preventDefault();
      inputRef.current?.focus();
      inputRef.current?.select();
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  return (
    <header className="app-header" data-tauri-drag-region>
      <div className="brand-lockup" data-tauri-drag-region>
        <BrandMark className="brand-mark" title="ClipMaster Pro" />
        <div data-tauri-drag-region>
          <strong>ClipMaster</strong>
          <span>PRO</span>
        </div>
      </div>

      <div className="search-field">
        <Icon name="search" className="search-field__icon" />
        <input
          ref={inputRef}
          id={SEARCH_INPUT_ID}
          type="text"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          placeholder="搜索剪贴板历史…"
          spellCheck={false}
          autoComplete="off"
          className="search-field__input"
          aria-label="搜索"
        />
        {value.length > 0 && (
          <button
            type="button"
            onClick={() => {
              onChange("");
              inputRef.current?.focus();
            }}
            className="icon-button search-field__clear"
            aria-label="清空搜索"
          >
            <Icon name="close" />
          </button>
        )}
        {/* 两个独立键帽更像 macOS 原生控件（设计规范差异清单 #9）。
            开始输入后由 CSS 换成「清空」按钮，右侧空间两者不共存。 */}
        <span className="search-field__keys" aria-hidden>
          <kbd>⌘</kbd>
          <kbd>K</kbd>
        </span>
      </div>
      <button
        type="button"
        onClick={onOpenSettings}
        title="设置"
        aria-label="打开设置"
        className="icon-button header-settings"
      >
        <Icon name="settings" />
      </button>
    </header>
  );
}
