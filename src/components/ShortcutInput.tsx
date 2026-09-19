import { useCallback, useEffect, useRef, useState } from "react";

/**
 * 快捷键录入组件（FR-SET-003）。
 *
 * 用法类比 Java Swing 的 KeyListener：
 * 点击进入录入模式后，监听真实按键组合，松开时生成 accelerator 字符串。
 *
 * Tauri accelerator 格式举例：
 *   - "CmdOrCtrl+Shift+V"
 *   - "Alt+Space"
 *   - "" （空字符串 = 不绑定）
 */

interface ShortcutInputProps {
  value: string;
  onChange: (accelerator: string) => void;
  disabled?: boolean;
  error?: string | null;
}

const KEY_DISPLAY_MAP: Record<string, string> = {
  CmdOrCtrl: navigator.platform.includes("Mac") ? "⌘" : "Ctrl",
  Shift: "⇧",
  Alt: navigator.platform.includes("Mac") ? "⌥" : "Alt",
};

function toDisplayLabel(accelerator: string): string {
  if (!accelerator) return "未设置";
  return accelerator
    .split("+")
    .map((part) => KEY_DISPLAY_MAP[part] ?? part)
    .join(" + ");
}

function keyEventToAccelerator(e: KeyboardEvent): string | null {
  const modifiers: string[] = [];
  if (e.metaKey || e.ctrlKey) modifiers.push("CmdOrCtrl");
  if (e.shiftKey) modifiers.push("Shift");
  if (e.altKey) modifiers.push("Alt");

  const key = e.key;

  // 纯修饰键本身不构成有效快捷键
  if (["Meta", "Control", "Shift", "Alt", "CapsLock"].includes(key)) {
    return null;
  }

  if (modifiers.length === 0) return null;

  // 字母键统一大写
  let normalizedKey = key.length === 1 ? key.toUpperCase() : key;

  // 一些常用键名映射到 Tauri 能识别的名称
  const keyMap: Record<string, string> = {
    " ": "Space",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Escape: "Escape",
    Enter: "Enter",
    Backspace: "Backspace",
    Delete: "Delete",
    Tab: "Tab",
  };
  if (keyMap[key]) {
    normalizedKey = keyMap[key];
  }

  return [...modifiers, normalizedKey].join("+");
}

export function ShortcutInput({
  value,
  onChange,
  disabled,
  error,
}: ShortcutInputProps) {
  const [recording, setRecording] = useState(false);
  const [pendingKeys, setPendingKeys] = useState<string[]>([]);
  const containerRef = useRef<HTMLDivElement>(null);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      // Escape 退出录入模式
      if (e.key === "Escape") {
        setRecording(false);
        setPendingKeys([]);
        return;
      }

      // Backspace/Delete 清除快捷键
      if (
        (e.key === "Backspace" || e.key === "Delete") &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.shiftKey &&
        !e.altKey
      ) {
        onChange("");
        setRecording(false);
        setPendingKeys([]);
        return;
      }

      // 实时显示正在按下的修饰键
      const mods: string[] = [];
      if (e.metaKey || e.ctrlKey) mods.push("CmdOrCtrl");
      if (e.shiftKey) mods.push("Shift");
      if (e.altKey) mods.push("Alt");
      setPendingKeys(mods);

      const accel = keyEventToAccelerator(e);
      if (accel) {
        onChange(accel);
        setRecording(false);
        setPendingKeys([]);
      }
    },
    [onChange],
  );

  const handleKeyUp = useCallback(() => {
    setPendingKeys([]);
  }, []);

  useEffect(() => {
    if (!recording) return;
    window.addEventListener("keydown", handleKeyDown, true);
    window.addEventListener("keyup", handleKeyUp, true);
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
      window.removeEventListener("keyup", handleKeyUp, true);
    };
  }, [recording, handleKeyDown, handleKeyUp]);

  // 点击外部退出录入
  useEffect(() => {
    if (!recording) return;
    function handleClickOutside(e: MouseEvent) {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setRecording(false);
        setPendingKeys([]);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [recording]);

  const displayText = recording
    ? pendingKeys.length > 0
      ? pendingKeys.map((k) => KEY_DISPLAY_MAP[k] ?? k).join(" + ") + " + …"
      : "请按下快捷键组合…"
    : toDisplayLabel(value);

  return (
    <div ref={containerRef} className="mt-1">
      <button
        type="button"
        disabled={disabled}
        onClick={() => {
          if (!disabled) setRecording(true);
        }}
        className={`w-full rounded-md border px-3 py-1.5 text-left text-sm transition-colors
          ${
            recording
              ? "border-[var(--cm-accent)] text-[var(--cm-accent-text)]"
              : "border-[var(--cm-line)] bg-transparent text-neutral-800 hover:border-[var(--cm-line-strong)] dark:border-white/10 "
          }
          ${disabled ? "cursor-not-allowed opacity-50" : "cursor-pointer"}
        `}
      >
        {displayText}
      </button>
      {recording && (
        <p className="mt-1 text-[10px] text-[var(--cm-fg-faint)]">
          按 Esc 取消 · 按 Delete 清除快捷键
        </p>
      )}
      {error && <p className="mt-1 text-xs text-[var(--cm-danger)]">{error}</p>}
    </div>
  );
}
