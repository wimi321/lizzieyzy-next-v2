import { type FormEvent, type ReactNode, useEffect, useState } from "react";
import type { FlashAnalysisSettings, WholeGameScope } from "@/api/backend";
import { rangeScope } from "./range";
import type { BatchLabels } from "./labels";

function Shell({ title, testId, close, onClose, children }: { title: string; testId: string; close: string; onClose(): void; children: ReactNode }) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog flash-dialog" role="dialog" aria-modal="true" aria-labelledby={`${testId}-title`} data-testid={testId}>
        <header className="dialog-header">
          <h2 id={`${testId}-title`}>{title}</h2>
          <button type="button" className="dialog-close" aria-label={close} onClick={onClose}>
            ×
          </button>
        </header>
        {children}
      </section>
    </div>
  );
}

/** Java's 局部精析: a first and a last move, either of which may be left blank. */
export function FlashRangeDialog({ labels, onStart, onClose }: { labels: BatchLabels; onStart(scope: WholeGameScope): void; onClose(): void }) {
  const [first, setFirst] = useState("");
  const [last, setLast] = useState("");
  const scope = rangeScope(first, last);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (scope) onStart(scope);
  };
  return (
    <Shell title={labels.rangeTitle} testId="flash-range" close={labels.close} onClose={onClose}>
      <form className="flash-form" noValidate onSubmit={submit}>
        <label className="flash-field">
          <span>{labels.startMove}</span>
          <input className="text-input" inputMode="numeric" data-testid="flash-first" value={first} aria-invalid={scope === null} onChange={(event) => setFirst(event.target.value)} />
        </label>
        <label className="flash-field">
          <span>{labels.endMove}</span>
          <input className="text-input" inputMode="numeric" data-testid="flash-last" value={last} aria-invalid={scope === null} onChange={(event) => setLast(event.target.value)} />
        </label>
        <p className="settings-hint">{scope === null ? labels.rangeInvalid : labels.rangeNotice}</p>
        <footer className="dialog-footer settings-footer">
          <button type="button" className="secondary-button" onClick={onClose}>
            {labels.cancel}
          </button>
          <button type="submit" className="primary-button" data-testid="flash-range-start" disabled={scope === null}>
            {labels.start}
          </button>
        </footer>
      </form>
    </Shell>
  );
}

const MAX_VISITS = 1_000_000;

/** Java's 精析设置: one search depth for every 精析 entry, and whether finished positions are redone. */
export function FlashSettingsDialog({
  settings,
  labels,
  onSave,
  onClose,
}: {
  settings: Required<FlashAnalysisSettings>;
  labels: BatchLabels;
  /** `redo` also runs the last 精析 again with the new settings. */
  onSave(settings: Required<FlashAnalysisSettings>, redo: boolean): void;
  onClose(): void;
}) {
  const [visits, setVisits] = useState(String(settings.visits));
  const [override, setOverride] = useState(settings.always_override);
  const trimmed = visits.trim();
  const value = /^\d+$/.test(trimmed) ? Number(trimmed) : null;
  const valid = value !== null && value >= 1 && value <= MAX_VISITS;
  const save = (redo: boolean) => {
    if (valid && value !== null) onSave({ visits: value, always_override: override }, redo);
  };
  return (
    <Shell title={labels.settingsTitle} testId="flash-settings" close={labels.close} onClose={onClose}>
      <form
        className="flash-form"
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          save(false);
        }}
      >
        <label className="flash-field">
          <span>{labels.settingsVisits}</span>
          <input className="text-input" inputMode="numeric" data-testid="flash-visits" value={visits} aria-invalid={!valid} onChange={(event) => setVisits(event.target.value)} />
        </label>
        <p className="settings-hint">{valid ? labels.settingsVisitsHint : labels.invalidVisits}</p>
        <label className="settings-check">
          <input type="checkbox" data-testid="flash-override" checked={override} onChange={(event) => setOverride(event.target.checked)} />
          <span>{labels.override}</span>
        </label>
        <footer className="dialog-footer settings-footer">
          <button type="button" className="secondary-button" data-testid="flash-settings-redo" disabled={!valid} onClick={() => save(true)}>
            {labels.confirmAndRedo}
          </button>
          <button type="submit" className="primary-button" data-testid="flash-settings-save" disabled={!valid}>
            {labels.confirm}
          </button>
        </footer>
      </form>
    </Shell>
  );
}
