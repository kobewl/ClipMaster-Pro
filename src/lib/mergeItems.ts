import type { ClipboardItem } from "@/types/clipboard";

/** 合并结果：文本 + 被跳过的图片条数。 */
export interface MergeResult {
  text: string;
  /** 因为不是文本而没被并进去的条数。 */
  skipped: number;
}

/**
 * 把选中的条目合并成一段文本。
 *
 * 规则（界面上也写明了，不能让用户猜）：
 * 1. 按**列表显示顺序**拼接，与勾选顺序无关 —— 这样结果可预期；
 * 2. 用换行连接，不加分隔符也不加编号（叫「合并」就该是原样拼接）；
 * 3. **跳过一切非文本条目**（图片 / HTML / 文件）：图片和文件的 content_text
 *    是本地路径，HTML 是标签源码，拼进纯文本里要么毫无意义要么不可读。
 *
 * @param items 当前列表（已经是显示顺序）
 * @param selected 选中的 id 集合
 */
export function mergeSelectedItems(
  items: ClipboardItem[],
  selected: ReadonlySet<string>,
): MergeResult {
  const parts: string[] = [];
  let skipped = 0;

  for (const item of items) {
    if (!selected.has(item.id)) continue;
    if (item.content_type !== "text") {
      skipped += 1;
      continue;
    }
    parts.push(item.content_text);
  }

  return { text: parts.join("\n"), skipped };
}

/** 「约 N 字」的统计口径与预览弹窗保持一致，用字符数而不是字节数。 */
export function countChars(text: string): number {
  return text.length;
}
