import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * 主窗口当前是否不可见。
 *
 * 不可见时列表刷新纯属浪费（用户看不到，回来时还得再刷一次），调用方据此只记 dirty。
 * 先看 `document.visibilityState`（同步、零 IPC）；WebKit 在窗口隐藏时不一定把它置成
 * hidden，所以再问一次 Tauri 的 `isVisible()`。任何异常都按「可见」处理 ——
 * 判错了也只是照常刷新，和原来的行为一样，不会更差。
 */
export async function isWindowHidden(): Promise<boolean> {
  if (document.visibilityState === "hidden") return true;
  try {
    return !(await getCurrentWindow().isVisible());
  } catch {
    return false;
  }
}

/**
 * 窗口重新显示时回调。返回同步的清理函数。
 *
 * 两个触发源：
 * - `visibilitychange` 变成 visible；
 * - Tauri 的 `onFocusChanged` 收到 true。后端每个显示窗口的入口
 *   （lib.rs:62/224、shortcut.rs:67、tray.rs:127）都是 `show()` 紧跟 `set_focus()`，
 *   所以「获得焦点」可以当作「重新显示」的可靠信号。
 *
 * 失焦不会隐藏窗口（lib.rs 只记下前台应用），可见但没聚焦时调用方照常刷新，
 * 不依赖这里。两个触发源可能对同一次显示都触发，调用方要自己保证幂等（例如靠 dirty 标记）。
 */
export function onWindowShown(handler: () => void): () => void {
  const handleVisibility = () => {
    if (document.visibilityState === "visible") handler();
  };
  document.addEventListener("visibilitychange", handleVisibility);

  let pendingUnlisten: Promise<UnlistenFn> | null = null;
  try {
    pendingUnlisten = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) handler();
    });
  } catch {
    // 非 Tauri 环境（或窗口 API 不可用）：只靠 visibilitychange。
    pendingUnlisten = null;
  }

  return () => {
    document.removeEventListener("visibilitychange", handleVisibility);
    pendingUnlisten?.then((unlisten) => unlisten()).catch(() => {});
  };
}
