import { type FormEvent, useEffect, useState } from "react";
import type { CaptureSettings } from "@/api/backend";
import step1 from "../assets/java/capture1.png?no-inline";
import step2 from "../assets/java/capture2.png?no-inline";
import step3 from "../assets/java/capture3.png?no-inline";
import type { CaptureLabels } from "./CapturePage";
import { CAPTURE_DEFAULTS, CAPTURE_FIELDS as FIELDS, type CaptureField as Field, describe, fill } from "./values";

function useEscape(onClose: () => void) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
}

function Header({ title, close, testId, onClose }: { title: string; close: string; testId: string; onClose(): void }) {
  return (
    <header className="dialog-header">
      <h2>{title}</h2>
      <button type="button" className="dialog-close" aria-label={close} data-testid={`${testId}-close`} onClick={onClose}>
        ×
      </button>
    </header>
  );
}

/** Java's 抓取死活题 window: three steps with a picture each, 参数设置 and 开始抓取. */
export function CaptureInstructions({ labels, onStart, onSettings, onClose }: { labels: CaptureLabels; onStart(): void; onSettings(): void; onClose(): void }) {
  useEscape(onClose);
  const steps = [
    [labels.step1, step1],
    [labels.step2, step2],
    [labels.step3, step3],
  ] as const;
  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog capture-dialog" role="dialog" aria-modal="true" aria-label={labels.title} data-testid="capture-dialog">
        <Header title={labels.title} close={labels.close} testId="capture-dialog" onClose={onClose} />
        <ol className="capture-steps">
          {steps.map(([text, picture], index) => (
            <li key={index}>
              <span>{text}</span>
              <img src={picture} alt="" draggable={false} />
            </li>
          ))}
        </ol>
        <footer className="dialog-footer capture-footer">
          <button type="button" className="secondary-button" data-testid="capture-settings" onClick={onSettings}>
            {labels.settings}
          </button>
          <span className="capture-tip">{labels.tip}</span>
          <button type="button" className="primary-button" data-testid="capture-start" onClick={onStart}>
            {labels.start}
          </button>
        </footer>
      </section>
    </div>
  );
}

/** The five recognition thresholds, each checked against its range before saving. */
export function CaptureSettingsDialog({
  settings,
  labels,
  onSave,
  onClose,
}: {
  settings: CaptureSettings;
  labels: CaptureLabels;
  /** Saves; rejects with the reason when Rust refuses. */
  onSave(settings: Required<CaptureSettings>): Promise<void>;
  onClose(): void;
}) {
  useEscape(onClose);
  const [values, setValues] = useState<Record<Field, string>>(() => {
    const filled = { ...CAPTURE_DEFAULTS, ...settings };
    return Object.fromEntries(FIELDS.map(({ key }) => [key, String(filled[key])])) as Record<Field, string>;
  });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const parsed = {} as Required<CaptureSettings>;
    for (const { key, min, max } of FIELDS) {
      const text = values[key].trim();
      const value = Number(text);
      if (!/^\d+$/.test(text) || value < min || value > max) {
        setError(fill(labels.rangeError, { field: labels.fields[key], min, max }));
        return;
      }
      parsed[key] = value;
    }
    setBusy(true);
    setError(null);
    onSave(parsed).catch((reason: unknown) => {
      setBusy(false);
      setError(fill(labels.saveFailed, { reason: describe(reason) }));
    });
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog capture-settings-dialog" role="dialog" aria-modal="true" aria-label={labels.settingsTitle} data-testid="capture-settings-dialog">
        <Header title={labels.settingsTitle} close={labels.close} testId="capture-settings" onClose={onClose} />
        <form className="capture-settings-form" noValidate onSubmit={submit}>
          {FIELDS.map(({ key, min, max }) => (
            <label key={key} className="capture-field">
              <span>{labels.fields[key]}</span>
              <input
                className="text-input"
                inputMode="numeric"
                data-testid={`capture-field-${key}`}
                value={values[key]}
                onChange={(event) => setValues({ ...values, [key]: event.target.value })}
              />
              <span className="capture-range">{fill(labels.range, { min, max })}</span>
            </label>
          ))}
          {error ? (
            <p className="dialog-error" role="alert" data-testid="capture-settings-error">
              {error}
            </p>
          ) : null}
          <footer className="dialog-footer capture-footer">
            <button
              type="button"
              className="secondary-button"
              data-testid="capture-defaults"
              onClick={() => setValues(Object.fromEntries(FIELDS.map(({ key }) => [key, String(CAPTURE_DEFAULTS[key])])) as Record<Field, string>)}
            >
              {labels.defaults}
            </button>
            <span className="capture-tip" />
            <button type="button" className="secondary-button" data-testid="capture-settings-cancel" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="capture-settings-save" disabled={busy}>
              {labels.confirm}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}

/** macOS without 屏幕录制: where to allow it, a button that opens the place, and 重试. */
export function CapturePermissionDialog({ labels, onOpenSettings, onRetry, onClose }: { labels: CaptureLabels; onOpenSettings(): void; onRetry(): void; onClose(): void }) {
  useEscape(onClose);
  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog capture-permission-dialog" role="dialog" aria-modal="true" aria-label={labels.permissionTitle} data-testid="capture-permission">
        <Header title={labels.permissionTitle} close={labels.close} testId="capture-permission" onClose={onClose} />
        <p>{labels.permissionMessage}</p>
        <p className="capture-tip">{labels.permissionRestart}</p>
        <footer className="dialog-footer capture-footer">
          <button type="button" className="secondary-button" data-testid="capture-open-settings" onClick={onOpenSettings}>
            {labels.openSettings}
          </button>
          <span className="capture-tip" />
          <button type="button" className="primary-button" data-testid="capture-retry" onClick={onRetry}>
            {labels.retry}
          </button>
        </footer>
      </section>
    </div>
  );
}
