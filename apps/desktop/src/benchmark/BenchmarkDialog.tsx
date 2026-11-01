import { useEffect } from "react";
import type { BenchmarkMeasurement, BenchmarkStatus } from "@/api/backend";

export interface BenchmarkLabels {
  title: string;
  subtitle: string;
  close: string;
  model: string;
  searchSpeed: string;
  searchSpeedHint: string;
  nnSpeed: string;
  nnSpeedHint: string;
  perSecond: string;
  threads: string;
  threadCount: string;
  batch: string;
  measuredAt: string;
  notRun: string;
  done: string;
  cancelled: string;
  failed: string;
  progress: string;
  starting: string;
  tried: string;
  explanation: string;
  duration: string;
  b11: string;
  start: string;
  rerun: string;
  stop: string;
  reset: string;
}

export interface BenchmarkDialogProps {
  status: BenchmarkStatus | null;
  /** The current engine's model, as the menu names it. */
  model: string | null;
  labels: BenchmarkLabels;
  onStart(): void;
  onStop(): void;
  onReset(): void;
  onClose(): void;
}

function speed(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return "—";
  return value >= 100 ? String(Math.round(value)) : value.toFixed(1);
}

/**
 * LizzieYzy Next's 性能优化 page: KataGo's own benchmark, started by the user, picks the
 * search threads (and the batch size it recommends); the window shows what it measured.
 */
export function BenchmarkDialog({ status, model, labels, onStart, onStop, onReset, onClose }: BenchmarkDialogProps) {
  const running = status?.phase === "Running";
  // Closing never stops a run: the menu strip's tray keeps showing it.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const result = status?.last_result ?? null;
  const progress = status?.progress ?? null;
  const measurements = status?.measurements ?? [];
  const fastest = measurements.reduce<BenchmarkMeasurement | null>(
    (top, row) => (top === null || row.visits_per_second > top.visits_per_second ? row : top),
    null,
  );
  // KataGo may recommend fewer threads than the fastest row: more threads cost search quality.
  const chosen = result ? (measurements.find((row) => row.threads === result.recommended_threads) ?? null) : fastest;
  const state = running
    ? null
    : status?.phase === "Failed"
      ? labels.failed
      : status?.phase === "Cancelled"
        ? labels.cancelled
        : result
          ? labels.done
          : labels.notRun;

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog benchmark-dialog" role="dialog" aria-modal="true" aria-labelledby="benchmark-title" data-testid="benchmark-dialog">
        <header className="dialog-header">
          <div>
            <h2 id="benchmark-title">{labels.title}</h2>
            <p className="benchmark-subtitle">{labels.subtitle}</p>
          </div>
          <button type="button" className="dialog-close" aria-label={labels.close} data-testid="benchmark-close" onClick={onClose}>
            ×
          </button>
        </header>

        <p className="benchmark-model">
          <span>{labels.model}</span>
          <strong data-testid="benchmark-model">{model ?? "—"}</strong>
          {state ? (
            <em className="benchmark-state" data-testid="benchmark-state" data-phase={status?.phase ?? "Idle"}>
              {state}
            </em>
          ) : null}
        </p>

        <div className="benchmark-metrics">
          <div className="benchmark-metric primary" title={labels.searchSpeedHint}>
            <span className="benchmark-metric-label">{labels.searchSpeed}</span>
            <strong data-testid="benchmark-search-speed">
              {speed(result?.visits_per_second)}
              <small>{labels.perSecond}</small>
            </strong>
            <span className="benchmark-metric-hint">{labels.searchSpeedHint}</span>
          </div>
          <div className="benchmark-metric" title={labels.nnSpeedHint}>
            <span className="benchmark-metric-label">{labels.nnSpeed}</span>
            <strong data-testid="benchmark-nn-speed">
              {speed(result?.nn_evals_per_second)}
              <small>{labels.perSecond}</small>
            </strong>
            <span className="benchmark-metric-hint">{labels.nnSpeedHint}</span>
          </div>
        </div>

        {result ? (
          <dl className="benchmark-facts" data-testid="benchmark-facts">
            <div>
              <dt>{labels.threads}</dt>
              <dd>{result.recommended_threads}</dd>
            </div>
            {result.nn_max_batch_size ? (
              <div>
                <dt>{labels.batch}</dt>
                <dd>{result.nn_max_batch_size}</dd>
              </div>
            ) : null}
            <div>
              <dt>{labels.measuredAt}</dt>
              <dd>{new Date(result.measured_at_unix_ms).toLocaleString()}</dd>
            </div>
          </dl>
        ) : null}

        {running ? (
          <div className="benchmark-run" data-testid="benchmark-progress">
            <div className="progress-bar" aria-hidden="true">
              <span style={{ width: progress ? `${(progress.completed_positions / Math.max(1, progress.total_positions)) * 100}%` : "0%" }} />
            </div>
            <p>
              {progress
                ? labels.progress
                    .replace("{threads}", String(progress.threads))
                    .replace("{done}", String(progress.completed_positions))
                    .replace("{total}", String(progress.total_positions))
                : labels.starting}
            </p>
          </div>
        ) : null}

        {measurements.length > 0 ? (
          <div className="benchmark-tried" data-testid="benchmark-tried">
            <span className="strength-section-label">{labels.tried}</span>
            <ol>
              {measurements.map((row) => (
                <li key={`${row.threads}-${row.visits_per_second}`} data-chosen={row === chosen}>
                  <span>{labels.threadCount.replace("{n}", String(row.threads))}</span>
                  <i style={{ width: fastest ? `${(row.visits_per_second / fastest.visits_per_second) * 100}%` : "0%" }} aria-hidden="true" />
                  <b>
                    {speed(row.visits_per_second)} {labels.perSecond}
                  </b>
                </li>
              ))}
            </ol>
          </div>
        ) : null}

        {status?.phase === "Failed" && status.error ? (
          <div className="dialog-error" role="alert" data-testid="benchmark-error">
            <pre>{status.error}</pre>
          </div>
        ) : null}

        <p className="benchmark-note">{labels.explanation}</p>
        {!result && !running ? <p className="benchmark-note">{labels.duration}</p> : null}
        {(result?.canonical_model_name ?? model ?? "").includes("b11") ? <p className="benchmark-note">{labels.b11}</p> : null}

        <footer className="dialog-footer benchmark-footer">
          {result && !running ? (
            <button type="button" className="secondary-button" data-testid="benchmark-reset" onClick={onReset}>
              {labels.reset}
            </button>
          ) : null}
          {running ? (
            <button type="button" className="secondary-button" data-testid="benchmark-stop" onClick={onStop}>
              {labels.stop}
            </button>
          ) : (
            <button type="button" className="primary-button" data-testid="benchmark-start" onClick={onStart}>
              {result ? labels.rerun : labels.start}
            </button>
          )}
        </footer>
      </section>
    </div>
  );
}
