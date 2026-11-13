import { useEffect, useState } from "react";
import type { Backend, FullLogStatus } from "@/api/backend";

export interface DiagnosticsLabels {
  title: string;
  intro: string;
  logs: string;
  open: string;
  fullLog: string;
  fullLogHint: string;
  fullLogOn: string;
  fullLogOff: string;
  start: string;
  stop: string;
  bundle: string;
  bundleHint: string;
  export: string;
  exporting: string;
  exported: string;
  reveal: string;
  failed: string;
  close: string;
}

type DiagnosticsBackend = Pick<Backend, "logDirectory" | "fullLogStatus" | "exportDiagnostics" | "revealInFolder">;

function message(error: unknown): string {
  if (typeof error === "object" && error !== null && "data" in error && typeof (error as { data: unknown }).data === "string") return (error as { data: string }).data;
  if (typeof error === "object" && error !== null && "type" in error) return String((error as { type: unknown }).type);
  return error instanceof Error ? error.message : String(error);
}

const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;

/**
 * 帮助 → 诊断与日志: where the logs are, Java's timed full trace, and a diagnostic bundle to send
 * the maintainers. Rust assembles and sanitises the bundle; this only asks for it and shows it.
 */
export function DiagnosticsDialog({
  backend,
  fullLog,
  labels,
  onStart,
  onStop,
  onClose,
}: {
  backend: DiagnosticsBackend;
  fullLog: FullLogStatus;
  labels: DiagnosticsLabels;
  onStart(): void;
  onStop(): void;
  onClose(): void;
}) {
  const [directory, setDirectory] = useState<string | null>(null);
  const [bundle, setBundle] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Counted down here from the status Rust gave; Rust ends the trace on time by itself.
  const [left, setLeft] = useState(fullLog.remaining_seconds ?? 0);
  const [seen, setSeen] = useState(fullLog);
  if (seen !== fullLog) {
    setSeen(fullLog);
    setLeft(fullLog.remaining_seconds ?? 0);
  }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  useEffect(() => {
    let active = true;
    backend
      .logDirectory()
      .then((path) => active && setDirectory(path))
      .catch((reason: unknown) => active && setError(message(reason)));
    return () => {
      active = false;
    };
  }, [backend]);

  const on = fullLog.enabled === true && left > 0;
  useEffect(() => {
    if (!on) return;
    const timer = window.setInterval(() => setLeft((value) => Math.max(0, value - 1)), 1000);
    return () => window.clearInterval(timer);
  }, [on]);

  const reveal = (path: string) => {
    backend.revealInFolder(path).catch((reason: unknown) => setError(message(reason)));
  };
  const exportBundle = () => {
    setBusy(true);
    setError(null);
    backend
      .exportDiagnostics()
      .then(setBundle)
      .catch((reason: unknown) => setError(message(reason)))
      .finally(() => setBusy(false));
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog diagnostics-dialog" role="dialog" aria-modal="true" aria-labelledby="diagnostics-title" data-testid="diagnostics-dialog">
        <header className="dialog-header">
          <h2 id="diagnostics-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <p className="dialog-intro">{labels.intro}</p>

        <div className="settings-row">
          <span className="settings-label">{labels.logs}</span>
          <span className="diagnostics-path" title={directory ?? undefined} data-testid="diagnostics-log-dir">
            {directory ?? "…"}
          </span>
          <button type="button" className="secondary-button" data-testid="diagnostics-open-logs" disabled={!directory} onClick={() => directory && reveal(directory)}>
            {labels.open}
          </button>
        </div>

        <div className="settings-row">
          <span className="settings-label">{labels.fullLog}</span>
          <span className="diagnostics-state" data-testid="diagnostics-full-log" data-on={on || undefined}>
            {on ? labels.fullLogOn.replace("{time}", clock(left)) : labels.fullLogOff}
          </span>
          {on ? (
            <button type="button" className="secondary-button" data-testid="diagnostics-stop" onClick={onStop}>
              {labels.stop}
            </button>
          ) : (
            <button type="button" className="secondary-button" data-testid="diagnostics-start" onClick={onStart}>
              {labels.start}
            </button>
          )}
          <p className="settings-hint">{labels.fullLogHint}</p>
        </div>

        <div className="settings-row">
          <span className="settings-label">{labels.bundle}</span>
          <span />
          <button type="button" className="primary-button" data-testid="diagnostics-export" disabled={busy} onClick={exportBundle}>
            {busy ? labels.exporting : labels.export}
          </button>
          <p className="settings-hint">{labels.bundleHint}</p>
          {bundle ? (
            <p className="diagnostics-bundle" data-testid="diagnostics-bundle">
              <span title={bundle}>{labels.exported.replace("{file}", bundle.split(/[\\/]/).pop() ?? bundle)}</span>
              <button type="button" className="secondary-button" data-testid="diagnostics-reveal" onClick={() => reveal(bundle)}>
                {labels.reveal}
              </button>
            </p>
          ) : null}
        </div>

        {error ? (
          <div className="dialog-error" role="alert" data-testid="diagnostics-error">
            <pre>{labels.failed.replace("{reason}", error)}</pre>
          </div>
        ) : null}
      </section>
    </div>
  );
}
