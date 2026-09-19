interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

import { Icon } from "./Icon";

/**
 * 通用二次确认弹窗。用于 FR-MGT-005：清空全部历史时必须二次确认。
 *
 * Esc 关闭由 App 统一监听（避免多个弹层各自抢按键），这里只负责视觉与点击遮罩关闭。
 */
export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "确认",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  if (!open) return null;

  return (
    <div
      className="modal-backdrop cm-fade-in"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-dialog-title"
      aria-describedby="confirm-dialog-description"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
    >
      <div className="modal-card confirm-card cm-pop-in">
        <span className="confirm-card__icon"><Icon name="trash" /></span>
        <h2
          id="confirm-dialog-title"
          className="text-sm font-semibold text-[var(--cm-fg)]"
        >
          {title}
        </h2>
        <p
          id="confirm-dialog-description"
          className="mt-2 text-xs leading-relaxed text-[var(--cm-fg-muted)]"
        >
          {description}
        </p>
        <div className="modal-footer">
          <button
            type="button"
            onClick={onCancel}
            className="button button--secondary"
          >
            取消
          </button>
          <button
            type="button"
            onClick={onConfirm}
            autoFocus
            className="button button--danger"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
