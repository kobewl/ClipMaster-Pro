/**
 * 与 Rust 端 DTO 对齐的类型定义。
 * 字段命名使用 snake_case 以匹配 Serde 默认序列化，避免额外的映射层。
 * 参考文档：02-架构与设计/01-总体技术架构.md 第 5、7 节。
 */

export type ContentType = "text";

export interface ClipboardItem {
  id: string;
  content_type: ContentType;
  content_text: string;
  preview: string;
  is_favorite: boolean;
  created_at: string;
  updated_at: string;
  last_copied_at: string;
  source_app: string | null;
}

export interface ListQuery {
  favorites_only: boolean;
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
  /** 全局快捷键 accelerator 字符串，空字符串表示不绑定 */
  shortcut: string;
}

/**
 * 与 02-架构与设计/01-总体技术架构.md 第 7.3 节的 CommandError 契约保持一致。
 * 前端必须依据 code 处理业务状态，不解析 message 自然语言文本。
 */
export interface CommandError {
  code: string;
  message: string;
  retryable: boolean;
  details?: unknown;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}
