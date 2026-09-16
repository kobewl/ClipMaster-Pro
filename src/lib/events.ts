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
