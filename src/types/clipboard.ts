export type ContentType = "text" | "image" | "html" | "files";

export interface ClipboardItem {
  id: string;
  content_type: ContentType;
  content_text: string;
  preview: string;
  group_id: string | null;
  created_at: string;
  updated_at: string;
  last_copied_at: string;
  source_app: string | null;
  source_url: string | null;
}

export interface ClipGroup {
  id: string;
  name: string;
  color: string;
  sort_order: number;
  item_count: number;
}

export interface ListQuery {
  group_id: string | null;
  search: string | null;
  limit: number;
  offset: number;
}

export interface ListResult {
  items: ClipboardItem[];
  total: number;
}

export interface AppSettings {
  max_history: number;
  retention_days: number;
  capture_enabled: boolean;
  shortcut: string;
}

export interface CommandError {
  code: string;
  message: string;
  retryable: boolean;
  details?: unknown;
}

/** 检查更新的结果（后端 check_for_updates 命令）。 */
export interface UpdateStatus {
  current_version: string;
  update_available: boolean;
  latest_version: string | null;
  notes: string | null;
  /** 最新 release 的 GitHub 页面地址，「前往下载」直接打开它。 */
  release_url: string | null;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}

export const GROUP_COLORS = [
  "#EF4444", "#F97316", "#F59E0B", "#22C55E",
  "#06B6D4", "#3B82F6", "#8B5CF6", "#EC4899",
  "#6B7280",
] as const;
