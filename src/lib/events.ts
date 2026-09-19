import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ClipboardItem } from "@/types/clipboard";

export function onClipboardCaptured(
  handler: (item: ClipboardItem) => void,
): Promise<UnlistenFn> {
  return listen<ClipboardItem>("clipboard://captured", (e) => handler(e.payload));
}

export function onClipboardDeleted(
  handler: (id: string) => void,
): Promise<UnlistenFn> {
  return listen<string>("clipboard://deleted", (e) => handler(e.payload));
}

export function onClipboardUpdated(
  handler: (item: ClipboardItem) => void,
): Promise<UnlistenFn> {
  return listen<ClipboardItem>("clipboard://updated", (e) => handler(e.payload));
}

export function onClipboardCleared(
  handler: (deletedCount: number) => void,
): Promise<UnlistenFn> {
  return listen<number>("clipboard://cleared", (e) => handler(e.payload));
}

export function onGroupsChanged(
  handler: () => void,
): Promise<UnlistenFn> {
  return listen("groups://changed", () => handler());
}

export interface AppErrorPayload {
  code: string;
  message: string;
}

export function onAppError(
  handler: (error: AppErrorPayload) => void,
): Promise<UnlistenFn> {
  return listen<AppErrorPayload>("app://error", (e) => handler(e.payload));
}

/**
 * 托盘菜单（或其它后端入口）改了采集开关。
 * 设置面板与状态栏据此同步显示 —— settings 表是唯一真相源，这里只是镜像。
 */
export function onCaptureChanged(
  handler: (captureEnabled: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>("settings://capture-changed", (e) => handler(e.payload));
}

/** 托盘「打开设置」：后端已把窗口拉起并聚焦，前端只负责打开面板。 */
export function onOpenSettings(handler: () => void): Promise<UnlistenFn> {
  return listen("ui://open-settings", () => handler());
}

/** 应用内更新的下载进度（0-100；总数未知时后端发 0）。 */
export function onUpdateProgress(
  handler: (percent: number) => void,
): Promise<UnlistenFn> {
  return listen<number>("update-progress", (e) => handler(e.payload));
}

/** 更新包下载完毕、开始安装 —— 安装完应用会自动重启。 */
export function onUpdateInstalling(handler: () => void): Promise<UnlistenFn> {
  return listen("update-installing", () => handler());
}
