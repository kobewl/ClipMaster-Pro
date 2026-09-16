import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ClipboardItem } from "@/types/clipboard";

/**
 * Rust -> UI 的事件通道封装。
 * 事件名称与架构文档 7.2 节保持一致：clipboard://captured 等。
 */
export function onClipboardCaptured(
  handler: (item: ClipboardItem) => void,
): Promise<UnlistenFn> {
  return listen<ClipboardItem>("clipboard://captured", (event) => {
    handler(event.payload);
  });
}

export function onClipboardDeleted(
  handler: (id: string) => void,
): Promise<UnlistenFn> {
  return listen<string>("clipboard://deleted", (event) => {
    handler(event.payload);
  });
}

export function onClipboardUpdated(
  handler: (item: ClipboardItem) => void,
): Promise<UnlistenFn> {
  return listen<ClipboardItem>("clipboard://updated", (event) => {
    handler(event.payload);
  });
}

/**
 * 批量清空历史后发出，payload 为实际删除的条数。
 * 与 clipboard://deleted 区分：批量清空没有单个 id 可报告，
 * 因此使用独立事件而不是勉强套用 deleted 的 payload 形状。
 */
export function onClipboardCleared(
  handler: (deletedCount: number) => void,
): Promise<UnlistenFn> {
  return listen<number>("clipboard://cleared", (event) => {
    handler(event.payload);
  });
}

export interface AppErrorPayload {
  code: string;
  message: string;
}

export function onAppError(
  handler: (error: AppErrorPayload) => void,
): Promise<UnlistenFn> {
  return listen<AppErrorPayload>("app://error", (event) => {
    handler(event.payload);
  });
}
