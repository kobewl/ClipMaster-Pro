import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ClipboardItem,
  ListQuery,
  ListResult,
} from "@/types/clipboard";

/**
 * Tauri Command 封装层。
 * 只负责调用与类型转换，不承载业务规则（架构文档 3.2 节）。
 */
export const commands = {
  listClipboardItems(query: ListQuery): Promise<ListResult> {
    return invoke("list_clipboard_items", { query });
  },

  deleteClipboardItem(id: string): Promise<void> {
    return invoke("delete_clipboard_item", { id });
  },

  setFavorite(id: string, favorite: boolean): Promise<void> {
    return invoke("set_favorite", { id, favorite });
  },

  copyClipboardItem(id: string): Promise<void> {
    return invoke("copy_clipboard_item", { id });
  },

  clearHistory(keepFavorites: boolean): Promise<number> {
    return invoke("clear_history", { keepFavorites });
  },

  getSettings(): Promise<AppSettings> {
    return invoke("get_settings");
  },

  updateSettings(settings: AppSettings): Promise<AppSettings> {
    return invoke("update_settings", { settings });
  },

  setCaptureEnabled(enabled: boolean): Promise<AppSettings> {
    return invoke("set_capture_enabled", { enabled });
  },

  updateShortcut(shortcut: string): Promise<AppSettings> {
    return invoke("update_shortcut", { shortcut });
  },
};

export type { ClipboardItem };
