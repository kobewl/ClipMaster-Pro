// `?no-inline` 强制 Vite 把 SVG 落盘为独立文件而不是内联成 data: URI ——
// CSP 的 img-src 白名单没有 data:，内联版在 release 包里会被整个拦掉（白方块）。
import logoUrl from "@/assets/clipmaster-logo.svg?no-inline";

interface BrandMarkProps {
  className?: string;
  title?: string;
}

/**
 * ClipMaster Pro 定稿 Logo（docs/design/README.md 第七节）。
 *
 * 用 `<img>` 引入整份 SVG 而不是内联 path：Logo 的「好看」来自材质
 * （柔光渐变、纸张投影、1.25px 边缘高光，全是 SVG filter），
 * 内联到 React 里改不动也保不住；交给渲染器按原样画。
 *
 * ≥22px 用完整版（依赖 filter）；≤24px 的场景（托盘、菜单栏）
 * 应改用剪影版 `clipmaster-logo-small.svg` —— 目前界面里只有头部这一处。
 */
export function BrandMark({ className = "brand-mark", title }: BrandMarkProps) {
  return (
    <img
      src={logoUrl}
      className={className}
      alt={title ?? "ClipMaster Pro"}
      title={title}
      draggable={false}
    />
  );
}
