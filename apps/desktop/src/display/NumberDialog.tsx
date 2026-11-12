import { type FormEvent, useEffect, useState } from "react";

export interface NumberDialogLabels {
  confirm: string;
  cancel: string;
  close: string;
  invalid: string;
}

export interface NumberDialogProps {
  title: string;
  /** What the number means, shown above the field. */
  prompt: string;
  value: number;
  min: number;
  max: number;
  labels: NumberDialogLabels;
  testId: string;
  onConfirm(value: number): void;
  onClose(): void;
}

/** Java's small input boxes for one whole number: 最后 N 手, the next-move hint's visits. */
export function NumberDialog({ title, prompt, value, min, max, labels, testId, onConfirm, onClose }: NumberDialogProps) {
  const [text, setText] = useState(String(value));
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const trimmed = text.trim();
  const parsed = /^\d+$/.test(trimmed) ? Number(trimmed) : null;
  const chosen = parsed !== null && parsed >= min && parsed <= max ? parsed : null;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (chosen !== null) onConfirm(chosen);
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog edit-dialog" role="dialog" aria-modal="true" aria-labelledby={`${testId}-title`} data-testid={testId}>
        <header className="dialog-header">
          <h2 id={`${testId}-title`}>{title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <form className="edit-form" noValidate onSubmit={submit}>
          <label className="edit-field">
            <span>{prompt}</span>
            <input
              className="text-input"
              inputMode="numeric"
              autoFocus
              value={text}
              aria-invalid={chosen === null}
              data-testid={`${testId}-input`}
              onChange={(event) => setText(event.target.value)}
            />
          </label>
          {chosen === null ? <span className="edit-invalid">{labels.invalid.replace("{min}", String(min)).replace("{max}", String(max))}</span> : null}
          <footer className="dialog-footer">
            <button type="button" className="secondary-button" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" disabled={chosen === null} data-testid={`${testId}-ok`}>
              {labels.confirm}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
