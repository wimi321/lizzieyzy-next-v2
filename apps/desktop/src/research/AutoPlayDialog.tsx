import { type FormEvent, useEffect, useState } from "react";
import type en from "../locales/en.json";
import { intervalOf } from "./values";

type Labels = typeof en.research.autoPlay;

export interface AutoPlayDialogProps {
  intervalMs: number;
  labels: Labels;
  onStart(intervalMs: number): void;
  onClose(): void;
}

/** 设置自动播放: how many seconds the main board waits between moves; 确定 starts playing. */
export function AutoPlayDialog({ intervalMs, labels, onStart, onClose }: AutoPlayDialogProps) {
  const [text, setText] = useState(String(intervalMs / 1000));
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const chosen = intervalOf(text);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (chosen !== null) onStart(chosen);
  };
  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog edit-dialog autoplay-dialog" role="dialog" aria-modal="true" aria-labelledby="autoplay-title" data-testid="autoplay-dialog">
        <header className="dialog-header">
          <h2 id="autoplay-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <form className="edit-form" noValidate onSubmit={submit}>
          <label className="edit-field">
            <span>{labels.interval}</span>
            <input
              className="text-input"
              inputMode="decimal"
              autoFocus
              value={text}
              aria-invalid={chosen === null}
              data-testid="autoplay-interval"
              onChange={(event) => setText(event.target.value)}
            />
          </label>
          {chosen === null ? <span className="edit-invalid">{labels.invalid}</span> : null}
          <footer className="dialog-footer">
            <button type="button" className="secondary-button" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" disabled={chosen === null} data-testid="autoplay-ok">
              {labels.confirm}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
