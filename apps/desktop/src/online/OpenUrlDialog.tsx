import { type FormEvent, useEffect, useState } from "react";
import type { Backend } from "@/api/backend";
import type en from "../locales/en.json";
import { urlError } from "./format";

export interface OpenUrlDialogProps {
  backend: Pick<Backend, "openUrl">;
  labels: typeof en.online.url;
  /** What the dialog starts with, such as a link found on the clipboard. */
  initial?: string;
  onClose(): void;
}

/** 文件 → 打开在线链接: Rust fetches the link, Fox and Tencent share links included, and opens the game. */
export function OpenUrlDialog({ backend, labels, initial = "", onClose }: OpenUrlDialogProps) {
  const [url, setUrl] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const link = url.trim();
    if (!link || busy || !backend.openUrl) return;
    setBusy(true);
    setError(null);
    backend
      .openUrl(link)
      .then(onClose)
      .catch((failure: unknown) => {
        setBusy(false);
        setError(urlError(failure, labels.errors));
      });
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog edit-dialog url-dialog" role="dialog" aria-modal="true" aria-labelledby="url-title" data-testid="url-dialog">
        <header className="dialog-header">
          <h2 id="url-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.cancel} onClick={onClose}>
            ×
          </button>
        </header>
        <form className="edit-form" noValidate onSubmit={submit}>
          <label className="url-label" htmlFor="url-input">
            {labels.label}
          </label>
          <input
            id="url-input"
            className="text-input"
            data-testid="url-input"
            autoFocus
            spellCheck={false}
            value={url}
            placeholder={labels.placeholder}
            aria-invalid={error !== null}
            onChange={(event) => setUrl(event.target.value)}
          />
          <p className="url-hint">{labels.hint}</p>
          {error ? (
            <p className="dialog-error" role="alert" data-testid="url-error">
              {error}
            </p>
          ) : null}
          <footer className="dialog-footer">
            <button type="button" className="secondary-button" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="url-open" disabled={busy || !url.trim()}>
              {busy ? labels.opening : labels.open}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
