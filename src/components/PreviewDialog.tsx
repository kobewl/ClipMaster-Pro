import { useEffect, useMemo, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ClipboardItem } from "@/types/clipboard";
import { extractDomain, getAppIcon } from "@/lib/sourceIcons";
import { Icon } from "./Icon";

interface Props {
  item: ClipboardItem | null;
  /** 来源应用的真实图标路径（null 时退回 emoji / 图片图标）。 */
  iconSrc: string | null;
  onCopy: (id: string) => void;
  onPaste: (id: string) => void;
  onClose: () => void;
}

function formatFullTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/**
 * 「查看全部内容」弹窗。
 *
 * 列表里的正文只有两行，而且后端下发的 `preview` 字段在 240 字处就截断了；
 * 完整的 `content_text` 其实一直是传到前端的，只是界面里没地方显示它。
 * 这个弹窗就是那个地方 —— 顺带也解决了「图片只能看缩略图」的问题。
 */
export function PreviewDialog({ item, iconSrc, onCopy, onPaste, onClose }: Props) {
  const bodyRef = useRef<HTMLDivElement>(null);

  // 长文打开时从头开始看；不做滚动位置记忆，每次都是新的阅读。
  useEffect(() => {
    if (item) bodyRef.current?.scrollTo({ top: 0 });
  }, [item]);

  const meta = useMemo(() => {
    if (!item) return null;
    const isImage = item.content_type === "image";
    // 字数只对文本有意义；图片报「图片」两个字就够了。
    const size = isImage ? "图片" : `共 ${item.content_text.length} 字`;
    return {
      isImage,
      source: extractDomain(item.source_url) ?? item.source_app ?? "未知来源",
      time: formatFullTime(item.last_copied_at),
      size,
    };
  }, [item]);

  if (!item || !meta) return null;

  return (
    <div
      className="modal-backdrop cm-fade-in"
      role="dialog"
      aria-modal="true"
      aria-labelledby="preview-dialog-title"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal-card preview-card cm-pop-in">
        <header className="modal-header">
          <span className="modal-header__icon">
            {/* 真实图标 > 图片图标 > emoji，与列表行保持同一套兜底顺序 */}
            {iconSrc ? (
              <img src={convertFileSrc(iconSrc)} alt="" className="preview-card__appicon" />
            ) : meta.isImage ? (
              <Icon name="image" />
            ) : (
              <span>{getAppIcon(item.source_app)}</span>
            )}
          </span>
          <div>
            <h2 id="preview-dialog-title">{meta.source}</h2>
            <p>
              {meta.time} · {meta.size}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭"
            className="icon-button"
          >
            <Icon name="close" />
          </button>
        </header>

        <div ref={bodyRef} className="preview-body scrollbar-thin">
          {meta.isImage ? (
            <img
              src={convertFileSrc(item.content_text)}
              alt="剪贴板图片"
              className="preview-body__image"
            />
          ) : (
            <p className="preview-body__text">{item.content_text}</p>
          )}
        </div>

        <footer className="preview-foot">
          <span className="preview-foot__hint">
            {meta.isImage ? "点「粘贴」插入这张图" : "可以直接拖选文字"} · <kbd>Esc</kbd> 关闭
          </span>
          <div className="preview-foot__actions">
            <button
              type="button"
              onClick={() => onCopy(item.id)}
              className="button button--secondary"
            >
              复制
            </button>
            <button
              type="button"
              onClick={() => onPaste(item.id)}
              autoFocus
              className="button button--primary"
            >
              粘贴
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
