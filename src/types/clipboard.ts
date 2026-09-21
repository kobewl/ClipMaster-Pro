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
  /** 按内容类型筛选："text" | "image" | "html" | "files"。null = 全部。 */
  content_type: string | null;
  /** 预设时间档："today" | "week" | "month"。null = 全部时间。 */
  time_range: string | null;
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

export type AgentAction =
  | "summarize"
  | "translate_zh"
  | "explain"
  | "extract_tasks"
  | "format_json";

export interface AgentResult {
  request_id: string;
  action: string;
  title: string;
  content: string;
  provider: string;
  model: string;
  source_item_ids: string[];
}

/** Key 的来源：系统钥匙串（用户填的）或开发期环境变量。 */
export type AgentKeySource = "keychain" | "env";

/** 设置面板用的模型配置快照。**没有密钥本体**，只有掩码提示。 */
export interface AgentConfigInfo {
  configured: boolean;
  /** 形如 "••••••••abcd"，没有配置时为 null。 */
  key_hint: string | null;
  key_source: AgentKeySource;
  base_url: string;
  model: string;
  /** 地址是用户自己设的（false = 正在用内置默认值）。 */
  base_url_is_custom: boolean;
  /** 界面上显示的服务名（默认地址是 "DeepSeek"，自定义地址是真实主机名）。 */
  provider_label: string;
}

/**
 * 一条 AI 使用记录。**没有 prompt 和响应正文** —— 那些内容就在剪贴板历史里，
 * 审计再存一份等于把隐私面翻倍。
 */
export interface AgentRun {
  id: string;
  created_at: string;
  /** 机器可读的动作名（汇总统计用）。 */
  action: string;
  /** 界面直接显示的动作名。 */
  action_label: string;
  /** 发往的服务；null = 在本地就被拦下，什么都没发出去。 */
  provider: string | null;
  model: string | null;
  input_item_ids: string[];
  input_chars: number;
  status: "ok" | "error";
  error_code: string | null;
  duration_ms: number;
  output_chars: number | null;
}

/** 调用耗时：秒级以下给毫秒，之上给一位小数的秒。 */
export function formatRunDuration(ms: number): string {
  return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
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
