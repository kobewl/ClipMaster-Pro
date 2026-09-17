import { memo } from "react";

interface HighlightedTextProps {
  text: string;
  keyword: string;
}

interface Segment {
  text: string;
  hit: boolean;
}

/** 把关键词按空白拆成多个词，用户输入 "react hooks" 时两个词都会高亮。 */
function splitTerms(keyword: string): string[] {
  return keyword
    .split(/\s+/)
    .map((term) => term.trim())
    .filter((term) => term.length > 0);
}

/** 在 text 中找出所有命中区间，合并重叠部分后返回 [start, end) 列表。 */
function findRanges(text: string, terms: string[]): Array<[number, number]> {
  const haystack = text.toLowerCase();
  const ranges: Array<[number, number]> = [];

  for (const term of terms) {
    const needle = term.toLowerCase();
    let cursor = 0;
    // 单个词最多匹配 100 处，避免超长文本 + 极短关键词产生大量节点。
    while (ranges.length < 200) {
      const at = haystack.indexOf(needle, cursor);
      if (at === -1) break;
      ranges.push([at, at + needle.length]);
      cursor = at + needle.length;
    }
  }
  if (ranges.length === 0) return ranges;

  ranges.sort((a, b) => a[0] - b[0]);
  const merged: Array<[number, number]> = [];
  for (const range of ranges) {
    const last = merged[merged.length - 1];
    if (last && range[0] <= last[1]) {
      last[1] = Math.max(last[1], range[1]);
    } else {
      merged.push([range[0], range[1]]);
    }
  }
  return merged;
}

/**
 * 关键词高亮文本。
 *
 * 后端用的是 FTS/子串匹配，这里做的是「把用户输入的字面量标黄」——
 * 两者在中文分词场景下未必逐字对齐，所以找不到命中时原样返回，不做任何变形。
 */
export const HighlightedText = memo(function HighlightedText({
  text,
  keyword,
}: HighlightedTextProps) {
  const terms = splitTerms(keyword);
  if (terms.length === 0) return <>{text}</>;

  const ranges = findRanges(text, terms);
  if (ranges.length === 0) return <>{text}</>;

  const segments: Segment[] = [];
  let cursor = 0;
  for (const [start, end] of ranges) {
    if (start > cursor) segments.push({ text: text.slice(cursor, start), hit: false });
    segments.push({ text: text.slice(start, end), hit: true });
    cursor = end;
  }
  if (cursor < text.length) segments.push({ text: text.slice(cursor), hit: false });

  return (
    <>
      {segments.map((segment, index) =>
        segment.hit ? (
          <mark
            key={index}
            className="rounded-[3px] bg-amber-200/80 text-inherit dark:bg-amber-400/30 dark:text-amber-50"
          >
            {segment.text}
          </mark>
        ) : (
          <span key={index}>{segment.text}</span>
        ),
      )}
    </>
  );
});
