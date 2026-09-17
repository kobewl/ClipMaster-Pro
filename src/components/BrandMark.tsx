interface BrandMarkProps {
  className?: string;
  title?: string;
}

/** ClipMaster Pro 的可缩放品牌标记：剪贴板轮廓 + 向前流动的双轨。 */
export function BrandMark({ className = "h-8 w-8", title }: BrandMarkProps) {
  return (
    <svg
      className={className}
      viewBox="0 0 48 48"
      fill="none"
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
    >
      {title && <title>{title}</title>}
      <rect width="48" height="48" rx="14" fill="#5B5FEF" />
      <path
        d="M17 14.75A3.75 3.75 0 0 1 20.75 11h6.5A3.75 3.75 0 0 1 31 14.75V17h1.25A3.75 3.75 0 0 1 36 20.75v12.5A3.75 3.75 0 0 1 32.25 37h-16.5A3.75 3.75 0 0 1 12 33.25v-12.5A3.75 3.75 0 0 1 15.75 17H17v-2.25Z"
        fill="white"
        fillOpacity=".98"
      />
      <rect x="19" y="13" width="10" height="6" rx="3" fill="#C9CBFF" />
      <path
        d="M17.5 25.25h10.1l-2.15-2.15 2.05-2.05 5.65 5.65-5.65 5.65-2.05-2.05 2.15-2.15H17.5v-2.9Z"
        fill="#20C7A6"
      />
    </svg>
  );
}
