import {
  type FormEvent,
  type ReactNode,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";

interface DialogFrameProps {
  open: boolean;
  title: string;
  children: ReactNode;
  onCancel(): void;
}

export function DialogFrame({
  open,
  title,
  children,
  onCancel,
}: DialogFrameProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const previousActiveElement = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;

    const focusable = () =>
      Array.from(
        dialog.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((element) => !element.hasAttribute("disabled"));
    const focusFirst = () => {
      const first = focusable()[0];
      if (first) first.focus();
      else dialog.focus();
    };

    if (open) {
      previousActiveElement.current =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      if (!dialog.open) dialog.showModal();
      const focusTimer = window.setTimeout(focusFirst, 0);
      return () => window.clearTimeout(focusTimer);
    }

    if (dialog.open) dialog.close();
    const restore = previousActiveElement.current;
    previousActiveElement.current = null;
    if (restore?.isConnected) restore.focus();
  }, [open]);

  return (
    <dialog
      ref={ref}
      className="dialog"
      tabIndex={-1}
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
      onKeyDown={(event) => {
        if (event.key !== "Tab") return;
        const focusable = Array.from(
          event.currentTarget.querySelectorAll<HTMLElement>(
            'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
          ),
        ).filter((element) => !element.hasAttribute("disabled"));
        if (focusable.length === 0) {
          event.preventDefault();
          event.currentTarget.focus();
          return;
        }
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }}
    >
      <section className="dialog-panel">
        <h2 id={titleId}>{title}</h2>
        {children}
      </section>
    </dialog>
  );
}

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "Confirm",
  danger = false,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  danger?: boolean;
  onCancel(): void;
  onConfirm(): void | Promise<void>;
}) {
  const [pending, setPending] = useState(false);

  async function confirm() {
    setPending(true);
    try {
      await onConfirm();
    } finally {
      setPending(false);
    }
  }

  return (
    <DialogFrame open={open} title={title} onCancel={onCancel}>
      <p>{description}</p>
      <div className="dialog-actions">
        <button
          type="button"
          className="secondary-button"
          disabled={pending}
          onClick={onCancel}
        >
          Cancel
        </button>
        <button
          type="button"
          className={danger ? "danger-button" : "primary-button"}
          disabled={pending}
          onClick={() => void confirm()}
        >
          {pending ? "Working..." : confirmLabel}
        </button>
      </div>
    </DialogFrame>
  );
}

export function PromptDialog({
  open,
  title,
  label,
  confirmLabel,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  title: string;
  label: string;
  confirmLabel: string;
  onCancel(): void;
  onConfirm(value: string): void | Promise<void>;
}) {
  const [value, setValue] = useState("");
  const [pending, setPending] = useState(false);

  useEffect(() => {
    if (open) setValue("");
  }, [open]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const normalized = value.trim();
    if (!normalized) return;
    setPending(true);
    try {
      await onConfirm(normalized);
    } finally {
      setPending(false);
    }
  }

  return (
    <DialogFrame open={open} title={title} onCancel={onCancel}>
      <form className="dialog-form" onSubmit={(event) => void submit(event)}>
        <label>
          {label}
          <input
            autoFocus
            value={value}
            maxLength={120}
            onChange={(event) => setValue(event.target.value)}
          />
        </label>
        <div className="dialog-actions">
          <button
            type="button"
            className="secondary-button"
            disabled={pending}
            onClick={onCancel}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="primary-button"
            disabled={pending || !value.trim()}
          >
            {pending ? "Working..." : confirmLabel}
          </button>
        </div>
      </form>
    </DialogFrame>
  );
}
