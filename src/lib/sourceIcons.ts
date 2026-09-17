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
// 转成 PNG 缓存在 `$APPDATA/appicons`，这里只负责缓存「应用名 → 图标路径」，
// 避免同一批应用名被反复请求。

/** 已解析结果：拿到路径的存路径；确认系统里没有的存 null（不再重复请求）。 */
const resolvedIcons = new Map<string, string | null>();
/** 正在请求中的应用名，防止同一批名字被并发请求多次。 */
const inFlight = new Set<string>();

/** 已解析出的真实图标路径；返回 null 表示该用 emoji 兜底。 */
export function getSourceIconPath(appName: string | null): string | null {
  if (!appName) return null;
  return resolvedIcons.get(appName) ?? null;
}

/**
 * 批量解析来源应用图标，只请求没查过的应用名。
 *
 * @returns 是否有**新**图标可用 —— 调用方据此决定要不要重渲染列表。
 */
export async function prefetchSourceIcons(appNames: string[]): Promise<boolean> {
  const pending = appNames.filter(
    (name) => name.length > 0 && !resolvedIcons.has(name) && !inFlight.has(name),
  );
  if (pending.length === 0) return false;

  pending.forEach((name) => inFlight.add(name));
  try {
    const icons = await commands.getSourceIcons(pending);
    let changed = false;
    for (const name of pending) {
      const path = icons[name] ?? null;
      resolvedIcons.set(name, path);
      if (path) changed = true;
    }
    return changed;
  } catch {
    // 解析失败时不要把它记成「查过了」，下次列表刷新还会再试一次。
    pending.forEach((name) => resolvedIcons.delete(name));
    return false;
  } finally {
    pending.forEach((name) => inFlight.delete(name));
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
