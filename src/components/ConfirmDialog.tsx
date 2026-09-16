interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * 通用二次确认弹窗。用于 FR-MGT-005：清空全部历史时必须二次确认。
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
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-dialog-title"
      onKeyDown={(event) => {
        if (event.key === "Escape") onCancel();
      }}
    >
      <div className="w-72 rounded-lg bg-white p-4 shadow-lg dark:bg-neutral-800">
        <h2
          id="confirm-dialog-title"
          className="text-sm font-semibold text-neutral-800 dark:text-neutral-100"
        >
          {title}
        </h2>
        <p className="mt-2 text-xs text-neutral-500 dark:text-neutral-400">
          {description}
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-md px-3 py-1.5 text-xs text-neutral-600 hover:bg-black/5 dark:text-neutral-300 dark:hover:bg-white/10"
          >
            取消
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className="rounded-md bg-red-500 px-3 py-1.5 text-xs text-white hover:bg-red-600"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
