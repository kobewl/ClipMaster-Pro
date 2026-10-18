import type { MouseEvent } from "react";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";

/** 只把这几类链接交给系统打开，其余（javascript: 被 urlTransform 清空后的空串、相对路径等）一律忽略。 */
const OPENABLE_HREF = /^(https?:|mailto:)/i;

/**
 * 链接一律拦下默认跳转，交给 `openUrl` 用系统浏览器打开（和「前往下载」同一条路），
 * webview 自己永远不导航。
 */
const components: Components = {
  a({ href, children }) {
    const handleClick = (event: MouseEvent<HTMLAnchorElement>) => {
      event.preventDefault();
      if (href && OPENABLE_HREF.test(href)) void openUrl(href);
    };
    return (
      <a href={href} rel="noreferrer noopener" onClick={handleClick}>
        {children}
      </a>
    );
  },
};

/**
 * 设置里「发现新版本」下方的更新说明，内容来自 GitHub Release 正文（即 CHANGELOG.md 对应一节）。
 *
 * - 按 Markdown（含 GFM 表格、删除线等）渲染；不接 rehype-raw，`skipHtml` 直接丢掉正文里的原始 HTML；
 * - 图片不渲染：CSP 本来就拦远程图片，加载外链图片还会向第三方暴露用户；
 * - 链接走系统浏览器，见上面的 `components.a`。
 */
export function ReleaseNotes({ markdown }: { markdown: string }) {
  return (
    <section className="update-notes" aria-label="本版本更新说明">
      <Markdown
        remarkPlugins={[remarkGfm]}
        skipHtml
        disallowedElements={["img"]}
        components={components}
      >
        {markdown}
      </Markdown>
    </section>
  );
}
