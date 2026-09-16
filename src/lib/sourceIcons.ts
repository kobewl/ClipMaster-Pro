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

/** 从 URL 提取域名。 */
export function extractDomain(url: string | null): string | null {
  if (!url) return null;
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return null;
  }
}
