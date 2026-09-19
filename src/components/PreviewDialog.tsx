import { useEffect, useMemo, useRef } from "react";
import DOMPurify from "dompurify";
import { convertFileSrc } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ClipboardItem } from "@/types/clipboard";
import { extractDomain, getAppIcon } from "@/lib/sourceIcons";
import { Icon } from "./Icon";
import { ImageZoom } from "./ImageZoom";

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

/** 文件条目的 content_text 约定：绝对路径列表，用 \n 分隔（与后端一致）。 */
function splitFiles(filesText: string): string[] {
  return filesText.split("\n").map((path) => path.trim()).filter(Boolean);
}

function fileNameOf(path: string): string {
  const index = path.lastIndexOf("/");
  return index >= 0 ? path.slice(index + 1) : path;
}

function parentDirOf(path: string): string {
  const index = path.lastIndexOf("/");
  return index > 0 ? path.slice(0, index) : "";
}

/**
 * 取 HTML 的纯文本正文字数。走一次 DOM 解析（先消毒再量），
 * 不用正则数标签 —— 精确字符数对「共 N 字」这个口径是底线。
 */
function htmlPlainLength(html: string): number {
  const template = document.createElement("template");
  template.innerHTML = DOMPurify.sanitize(html);
  return (template.content.textContent ?? "").replace(/\s+/g, "").length;
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
    const isHtml = item.content_type === "html";
    const isFiles = item.content_type === "files";
    // 字数只对文本有意义；图片报「图片」，文件报个数，HTML 报去标签后的正文字数。
    const size = isImage
      ? "图片"
      : isFiles
        ? `${splitFiles(item.content_text).length} 个文件`
        : isHtml
          ? `共 ${htmlPlainLength(item.content_text)} 字`
          : `共 ${item.content_text.length} 字`;
    return {
      isImage,
      isHtml,
      isFiles,
      source: extractDomain(item.source_url) ?? item.source_app ?? "未知来源",
      time: formatFullTime(item.last_copied_at),
      size,
    };
  }, [item]);

  /**
   * 富文本**只在展示这一刻**消毒（DOMPurify 默认白名单，脚本/事件属性全剥掉）。
   * 数据库里的 content_text 永远是原文 —— 回写剪贴板时粘出去的仍是带格式的内容，
   * 消毒只对「渲染进界面」这一步负责，安全边界清晰。
   */
  const sanitizedHtml = useMemo(
    () => (meta?.isHtml ? DOMPurify.sanitize(item?.content_text ?? "") : null),
    [meta?.isHtml, item],
  );

  const filePaths = useMemo(
    () => (meta?.isFiles && item ? splitFiles(item.content_text) : []),
    [meta?.isFiles, item],
  );

  async function handleReveal(path: string) {
    try {
      await revealItemInDir(path);
    } catch {
      // 打不开 Finder 不致命：路径本身在界面上可见，用户能自己去找。
    }
  }

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
      <div
        className={`modal-card preview-card ${meta.isImage ? "preview-card--image" : ""} cm-pop-in`}
      >
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

        {meta.isImage ? (
          <div className="preview-image-wrap">
            <ImageZoom src={convertFileSrc(item.content_text)} alt="剪贴板图片" />
          </div>
        ) : meta.isFiles ? (
          <div ref={bodyRef} className="preview-body scrollbar-thin">
            <ul className="file-list">
              {filePaths.map((path) => (
                <li key={path} className="file-list__row">
                  <span className="file-list__icon" aria-hidden>📄</span>
                  <div className="file-list__text">
                    <span className="file-list__name">{fileNameOf(path)}</span>
                    <span className="file-list__dir">{parentDirOf(path)}</span>
                  </div>
                  <button
                    type="button"
                    onClick={() => void handleReveal(path)}
                    className="button button--secondary button--compact"
                    title="在 Finder 中显示该文件"
                  >
                    显示
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ) : meta.isHtml ? (
          <div ref={bodyRef} className="preview-body scrollbar-thin">
            <div
              className="preview-body__html"
              // sanitizedHtml 已过 DOMPurify 白名单；渲染原文在这里永远不出现。
              dangerouslySetInnerHTML={{ __html: sanitizedHtml ?? "" }}
            />
          </div>
        ) : (
          <div ref={bodyRef} className="preview-body scrollbar-thin">
            <p className="preview-body__text">{item.content_text}</p>
          </div>
        )}

        <footer className="preview-foot">
          <span className="preview-foot__hint">
            {meta.isImage
              ? "点图片可放大 · ⌘ 滚轮缩放"
              : meta.isHtml
                ? "富文本预览（消毒后渲染）· 可拖选"
                : meta.isFiles
                  ? "点「显示」在 Finder 中定位文件"
                  : "可以直接拖选文字"}{" "}
            · <kbd>Esc</kbd> 关闭
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
