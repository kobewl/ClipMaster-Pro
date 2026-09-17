import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ClipGroup,
  ClipboardItem,
  ListQuery,
  ListResult,
} from "@/types/clipboard";

export const commands = {
  listClipboardItems(query: ListQuery): Promise<ListResult> {
    return invoke("list_clipboard_items", { query });
  },
  deleteClipboardItem(id: string): Promise<void> {
    return invoke("delete_clipboard_item", { id });
  },
  setItemGroup(id: string, groupId: string | null): Promise<void> {
    return invoke("set_item_group", { id, groupId });
  },
  copyClipboardItem(id: string): Promise<void> {
    return invoke("copy_clipboard_item", { id });
  },
  pasteClipboardItem(id: string): Promise<void> {
    return invoke("paste_clipboard_item", { id });
  },
  /**
   * 解析来源应用的真实图标，返回 {应用名: PNG 绝对路径}。
   * 系统里找不到的应用不会出现在结果里（前端退回 emoji）。
   */
  getSourceIcons(apps: string[]): Promise<Record<string, string>> {
    return invoke("get_source_icons", { apps });
  },
  clearHistory(keepGrouped: boolean): Promise<number> {
    return invoke("clear_history", { keepGrouped });
  },

  // Groups
  listGroups(): Promise<ClipGroup[]> {
    return invoke("list_groups");
  },
  createGroup(dto: { name: string; color: string }): Promise<ClipGroup> {
    return invoke("create_group", { dto });
  },
  updateGroup(dto: { id: string; name: string; color: string }): Promise<ClipGroup> {
    return invoke("update_group", { dto });
  },
  deleteGroup(id: string): Promise<void> {
    return invoke("delete_group", { id });
  },

  // Settings
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
