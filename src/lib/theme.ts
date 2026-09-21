/** 外观主题偏好。`system` = 跟随 macOS 浅色/深色。 */
export type ThemePreference = "system" | "light" | "dark";

export const THEME_OPTIONS: ReadonlyArray<{
  value: ThemePreference;
  label: string;
}> = [
  { value: "system", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
] as const;

export function isThemePreference(value: string): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}

function resolveDark(preference: ThemePreference): boolean {
  if (preference === "dark") return true;
  if (preference === "light") return false;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

let systemMql: MediaQueryList | null = null;
let systemListener: ((event: MediaQueryListEvent) => void) | null = null;

/**
 * 把主题偏好落到 `<html>`：
 * - `data-theme` 记录用户选择（system / light / dark）
 * - `.dark` class 表示**实际**深色，供 CSS 变量与 Tailwind `dark:` 使用
 */
export function applyTheme(preference: ThemePreference): void {
  const root = document.documentElement;
  root.dataset.theme = preference;

  const dark = resolveDark(preference);
  root.classList.toggle("dark", dark);
  root.style.colorScheme = dark ? "dark" : "light";

  if (systemMql && systemListener) {
    systemMql.removeEventListener("change", systemListener);
  }
  systemMql = window.matchMedia("(prefers-color-scheme: dark)");
  systemListener = () => {
    if (root.dataset.theme === "system") {
      applyTheme("system");
    }
  };
  if (preference === "system") {
    systemMql.addEventListener("change", systemListener);
  }
}
