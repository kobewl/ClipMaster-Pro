import type { ClipboardItem, SourceIconRequest } from "@/types/clipboard";
import { commands } from "./commands";

/** 根据应用名返回图标 emoji。 */
export function getAppIcon(appName: string | null): string {
  if (!appName) return "📋";
  const n = appName.toLowerCase();
  if (n.includes("safari")) return "🧭";
  if (n.includes("chrome")) return "🌐";
  if (n.includes("firefox")) return "🦊";
  if (n.includes("edge")) return "🌊";
  if (n.includes("arc")) return "🌈";
  if (n.includes("brave")) return "🦁";
  if (n.includes("cursor") || n.includes("code")) return "💻";
  if (n.includes("terminal") || n.includes("iterm") || n.includes("warp")) return "⬛";
  if (n.includes("finder")) return "📁";
  if (n.includes("notes") || n.includes("备忘录")) return "📝";
  if (n.includes("wechat") || n.includes("微信")) return "💬";
  if (n.includes("slack") || n.includes("discord")) return "💬";
  if (n.includes("telegram")) return "✈️";
  if (n.includes("mail") || n.includes("outlook")) return "✉️";
  if (n.includes("preview") || n.includes("预览")) return "🖼";
  if (n.includes("pages") || n.includes("word")) return "📄";
  if (n.includes("numbers") || n.includes("excel")) return "📊";
  if (n.includes("keynote") || n.includes("powerpoint")) return "📽";
  if (n.includes("xcode")) return "🔨";
  if (n.includes("intellij") || n.includes("idea") || n.includes("webstorm") || n.includes("pycharm")) return "🧠";
  return "📋";
}

// ---------------------------------------------------------------------------
//  真实应用图标
// ---------------------------------------------------------------------------
//
// emoji 表只能覆盖常见应用，遇到表里没有的（比如 ZCode）就退化成一个通用图标，
// 用户看不出内容来自哪里。所以真正的图标由 Rust 侧从系统里取（NSWorkspace），
// 转成 PNG 缓存在 `$APPDATA/appicons`。
//
// 后端优先用复制时记下的 `.app` 路径和 bundle id 找应用，应用名只给没有这两项的
// 旧记录兜底。这里只负责缓存「来源 → 图标路径」：有 bundle id 按 bundle id 记，
// 否则按应用名记，避免同一个应用被反复请求。

/** 取图标需要的来源字段（`ClipboardItem` 的子集）。 */
export type SourceRef = Pick<ClipboardItem, "source_app" | "source_bundle_id" | "source_app_path">;

/** 来源在前端缓存里的键：有 bundle id 用 bundle id，否则用应用名；都没有返回 null。 */
export function sourceIconKey(ref: SourceRef | null | undefined): string | null {
  return ref?.source_bundle_id || ref?.source_app || null;
}

/** 已解析结果：拿到路径的存路径；确认系统里没有的存 null（不再重复请求）。 */
const resolvedIcons = new Map<string, string | null>();
/** 正在请求中的键，防止同一批来源被并发请求多次。 */
const inFlight = new Set<string>();

/** 已解析出的真实图标路径；返回 null 表示该用 emoji 兜底。 */
export function getSourceIconPath(ref: SourceRef | null | undefined): string | null {
  const key = sourceIconKey(ref);
  if (!key) return null;
  return resolvedIcons.get(key) ?? null;
}

/**
 * 批量解析来源应用图标，只请求没查过的来源（同一个键只发一次）。
 *
 * @returns 是否有**新**图标可用 —— 调用方据此决定要不要重渲染列表。
 */
export async function prefetchSourceIcons(refs: SourceRef[]): Promise<boolean> {
  const pending: { key: string; ref: SourceRef }[] = [];
  const seen = new Set<string>();
  for (const ref of refs) {
    const key = sourceIconKey(ref);
    if (!key || seen.has(key) || resolvedIcons.has(key) || inFlight.has(key)) continue;
    seen.add(key);
    pending.push({ key, ref });
  }
  if (pending.length === 0) return false;

  pending.forEach(({ key }) => inFlight.add(key));
  try {
    const requests: SourceIconRequest[] = pending.map(({ ref }) => ({
      name: ref.source_app ?? null,
      bundle_id: ref.source_bundle_id ?? null,
      app_path: ref.source_app_path ?? null,
    }));
    const paths = await commands.getSourceIcons(requests);
    let changed = false;
    pending.forEach(({ key }, index) => {
      // 结果按位置对应；长度不够或形状不对都按「找不到」处理。
      const path = (Array.isArray(paths) ? paths[index] : null) ?? null;
      resolvedIcons.set(key, path);
      if (path) changed = true;
    });
    return changed;
  } catch {
    // 解析失败时不要把它记成「查过了」，下次列表刷新还会再试一次。
    pending.forEach(({ key }) => resolvedIcons.delete(key));
    return false;
  } finally {
    pending.forEach(({ key }) => inFlight.delete(key));
  }
}

/** 从 URL 提取域名。 */
export function extractDomain(url: string | null): string | null {
  if (!url) return null;
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return null;
  }
}
