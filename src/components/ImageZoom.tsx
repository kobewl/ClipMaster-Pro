import { useCallback, useEffect, useMemo, useRef, useState } from "react";

interface Props {
  src: string;
  alt: string;
}

/**
 * 缩放档位。
 *
 * 用固定档位而不是连续缩放：每一步都可预期，点几下就知道自己在什么比例，
 * 也容易回到 100%。连续缩放（比如每次 ×1.1）会出现「放大了 37%」这种没意义的数字。
 */
const ZOOM_STEPS = [0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4] as const;

/**
 * 图片查看器：适应窗口 / 100% / 放大到 4 倍。
 *
 * 剪贴板里的截图常常是两三千像素宽，塞进弹窗里就成了缩略图，
 * 里面的字根本看不清 —— 这个组件的存在就是为了「看细节」。
 *
 * 三种操作方式，覆盖鼠标、触控板、键盘三类习惯：
 * - 点图片：在「适应窗口」和「100%」之间来回切（最常用，一个动作）
 * - 点 `−` / `+`：按档位缩放，`⌘` + 滚轮同样有效（触控板用户的直觉）
 * - 放大后拖拽：平移查看；直接用滚动条 / 双指滚动也可以
 */
export function ImageZoom({ src, alt }: Props) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null);
  const [viewport, setViewport] = useState({ w: 0, h: 0 });
  /** null 表示「适应窗口」；数字表示相对原图的比例（1 = 100%）。 */
  const [scale, setScale] = useState<number | null>(null);

  // 窗口缩放时要重算「适应」的比例，否则百分比会停在旧值上。
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect) setViewport({ w: rect.width, h: rect.height });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // 换一张图要回到「适应窗口」，否则会带着上一张的放大倍数，
  // 看起来就像新图莫名其妙被放大了。
  useEffect(() => {
    setScale(null);
    setNatural(null);
  }, [src]);

  /**
   * 「适应窗口」的比例。**不放大超过 100%** ——
   * 一张 40×40 的小图被拉满整个弹窗只会更糊，按原尺寸显示才是诚实的。
   */
  const fitScale = useMemo(() => {
    if (!natural || viewport.w === 0 || viewport.h === 0) return 1;
    return Math.min(viewport.w / natural.w, viewport.h / natural.h, 1);
  }, [natural, viewport]);

  const effective = scale ?? fitScale;
  const isFit = scale === null || Math.abs(scale - fitScale) < 0.001;

  // 内容是否超出视口 —— 只有超出时才允许拖动，否则「点一下切缩放」会被拖拽吃掉。
  // 视口本身尺寸不变时 ResizeObserver 不会触发，所以这里要跟着 effective 重算。
  const [overflowing, setOverflowing] = useState(false);
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const check = () =>
      setOverflowing(el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1);
    check();
    const observer = new ResizeObserver(check);
    observer.observe(el);
    return () => observer.disconnect();
  }, [effective, viewport]);

  const applyStep = useCallback(
    (direction: 1 | -1) => {
      // 用函数式更新读当前值，而不是闭包里的 effective：
      // 连续快速点两下 `+`，第二下可能还跑在第一次重渲染之前，
      // 读到旧值就只会放大一档（实测过）。
      setScale((current) => {
        const base = current ?? fitScale;
        // 找基准比例在档位表里的位置：放大取「比它大的第一个」，缩小取「比它小的最后一个」，
        // 这样即使当前是「适应窗口」算出来的非整数比例（比如 16%），点一下也能落到规整档位上。
        const candidates =
          direction > 0
            ? ZOOM_STEPS.filter((step) => step > base + 0.001)
            : [...ZOOM_STEPS].reverse().filter((step) => step < base - 0.001);
        const next = candidates[0];
        // 已在两端就停在原地。返回原值（含 null = 仍在适应窗口）不触发多余渲染。
        return next === undefined ? current : next;
      });
    },
    [fitScale],
  );

  const toggleFitAndActual = useCallback(() => {
    setScale(isFit ? 1 : null);
  }, [isFit]);

  // ⌘ + 滚轮缩放。用原生监听而不是 React 的 onWheel：后者是 passive 的，
  // 拦不住页面的默认滚动，缩放的同一时刻图片还会跟着滚。
  // `⌘` 滚轮会连续触发，`applyStep` 的函数式更新保证了每一发都按顺序生效。
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    function handleWheel(event: WheelEvent) {
      if (!event.metaKey && !event.ctrlKey) return;
      event.preventDefault();
      applyStep(event.deltaY < 0 ? 1 : -1);
    }
    el.addEventListener("wheel", handleWheel, { passive: false });
    return () => el.removeEventListener("wheel", handleWheel);
  }, [applyStep]);

  // 拖动平移。只在内容确实超出时才生效，否则会把「点一下切缩放」吃掉。
  const dragRef = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const [dragging, setDragging] = useState(false);

  function handlePointerDown(event: React.PointerEvent<HTMLDivElement>) {
    if (!overflowing) return;
    const el = viewportRef.current;
    if (!el) return;
    dragRef.current = {
      x: event.clientX,
      y: event.clientY,
      left: el.scrollLeft,
      top: el.scrollTop,
    };
    setDragging(true);
    el.setPointerCapture(event.pointerId);
  }

  function handlePointerMove(event: React.PointerEvent<HTMLDivElement>) {
    const drag = dragRef.current;
    const el = viewportRef.current;
    if (!drag || !el) return;
    el.scrollLeft = drag.left - (event.clientX - drag.x);
    el.scrollTop = drag.top - (event.clientY - drag.y);
  }

  function endDrag(event: React.PointerEvent<HTMLDivElement>) {
    if (!dragRef.current) return;
    dragRef.current = null;
    setDragging(false);
    viewportRef.current?.releasePointerCapture(event.pointerId);
  }

  /** 点图片：只在「没有拖动过」时才算点击，否则拖完手一松就会触发缩放。 */
  const movedRef = useRef(false);
  function handleClick() {
    if (movedRef.current) {
      movedRef.current = false;
      return;
    }
    toggleFitAndActual();
  }

  const displayPercent = Math.round(effective * 100);
  const canZoomIn = effective < ZOOM_STEPS[ZOOM_STEPS.length - 1] - 0.001;
  const canZoomOut = effective > ZOOM_STEPS[0] + 0.001;

  return (
    <div className="image-zoom">
      <div
        ref={viewportRef}
        className={`image-zoom__viewport scrollbar-thin ${
          overflowing ? (dragging ? "image-zoom__viewport--dragging" : "image-zoom__viewport--pannable") : ""
        }`}
        onPointerDown={handlePointerDown}
        onPointerMove={(event) => {
          if (dragRef.current) movedRef.current = true;
          handlePointerMove(event);
        }}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
      >
        <img
          src={src}
          alt={alt}
          draggable={false}
          onClick={handleClick}
          onLoad={(event) => {
            const image = event.currentTarget;
            setNatural({ w: image.naturalWidth, h: image.naturalHeight });
          }}
          className={`image-zoom__image ${scale === null ? "image-zoom__image--fit" : ""}`}
          style={
            scale === null || !natural
              ? undefined
              : { width: `${natural.w * scale}px`, height: `${natural.h * scale}px` }
          }
          title={isFit ? "点击放大到 100%" : "点击适应窗口"}
        />
      </div>

      <div className="image-zoom__bar">
        <button
          type="button"
          onClick={() => applyStep(-1)}
          disabled={!canZoomOut}
          aria-label="缩小"
          className="image-zoom__btn"
        >
          −
        </button>
        <button
          type="button"
          onClick={() => setScale(null)}
          title="适应窗口"
          className={`image-zoom__percent ${isFit ? "image-zoom__percent--fit" : ""}`}
        >
          {displayPercent}%
        </button>
        <button
          type="button"
          onClick={() => applyStep(1)}
          disabled={!canZoomIn}
          aria-label="放大"
          className="image-zoom__btn"
        >
          +
        </button>
        {natural && (
          <span className="image-zoom__size">
            {natural.w} × {natural.h}
          </span>
        )}
        <span className="image-zoom__hint">⌘ 滚轮缩放 · 拖动平移</span>
      </div>
    </div>
  );
}
