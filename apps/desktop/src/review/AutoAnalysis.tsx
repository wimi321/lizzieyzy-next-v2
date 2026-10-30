import { useEffect, useRef, useState } from "react";
import type { WholeGameProgress } from "@/api/backend";
import { Icon } from "../toolbar/icons";

/** Visits per move, as LizzieYzy Next's whole-game presets. */
const PRESETS = [
  { key: "quick", visits: 500 },
  { key: "standard", visits: 1000 },
  { key: "deep", visits: 3000 },
  { key: "professional", visits: 10_000 },
] as const;

export interface AutoAnalysisLabels {
  title: string;
  start: string;
  pause: string;
  resume: string;
  cancel: string;
  quick: string;
  standard: string;
  deep: string;
  professional: string;
  visitsPerMove: string;
  scope: string;
  running: string;
  paused: string;
  completed: string;
  cancelled: string;
  error: string;
}

export interface AutoAnalysisProps {
  progress: WholeGameProgress | null;
  /** Positions on the displayed line, root included: what a new run would cover. */
  positions: number;
  labels: AutoAnalysisLabels;
  onStart(visits: number): void;
  onPause(): void;
  onResume(): void;
  onCancel(): void;
}

/** The top-bar entry for whole-game analysis: a button at rest, a progress chip while it runs. */
export function AutoAnalysis({ progress, positions, labels, onStart, onPause, onResume, onCancel }: AutoAnalysisProps) {
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<{ top: number; right: number } | null>(null);
  const [visits, setVisits] = useState<number>(1000);
  const root = useRef<HTMLDivElement>(null);
  const status = progress?.status.type;
  const active = status === "Running" || status === "Paused";
  const share = progress && progress.total > 0 ? progress.completed / progress.total : 0;

  const toggle = () => {
    // Fixed position: the top bar clips overflow so narrow windows never overlap.
    const rect = root.current?.getBoundingClientRect();
    if (rect) setAnchor({ top: rect.bottom + 8, right: Math.max(8, window.innerWidth - rect.right) });
    setOpen((value) => !value);
  };

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!(event.target instanceof Node) || !root.current?.contains(event.target)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const outcome =
    status === "Completed"
      ? labels.completed.replace("{n}", String(progress?.total ?? 0))
      : status === "Cancelled"
        ? labels.cancelled
        : progress?.status.type === "Error"
          ? labels.error.replace("{reason}", progress.status.data.reason)
          : null;

  return (
    <div className="auto-analysis" ref={root}>
      {active && progress ? (
        <button
          type="button"
          className="chip progress-chip"
          data-testid="auto-analysis"
          data-state={status}
          aria-expanded={open}
          title={labels.title}
          onClick={toggle}
        >
          <span className="progress-fill" style={{ width: `${share * 100}%` }} aria-hidden="true" />
          <Icon name="bolt" />
          <strong>
            {progress.completed} / {progress.total}
          </strong>
        </button>
      ) : (
        <button
          type="button"
          className="icon-button"
          data-testid="auto-analysis"
          aria-expanded={open}
          title={labels.title}
          onClick={toggle}
        >
          <Icon name="bolt" />
          <span>{labels.title}</span>
        </button>
      )}
      {open ? (
        <div
          className="popover"
          role="dialog"
          aria-label={labels.title}
          data-testid="auto-analysis-panel"
          style={anchor ? { top: anchor.top, right: anchor.right } : undefined}
        >
          <h3 className="popover-title">{labels.title}</h3>
          {active && progress ? (
            <>
              <div className="progress-bar" aria-hidden="true">
                <span style={{ width: `${share * 100}%` }} />
              </div>
              <p className="popover-text">
                {status === "Paused" ? labels.paused : labels.running} · {progress.completed} / {progress.total} ·{" "}
                {labels.visitsPerMove.replace("{visits}", String(progress.visits))}
              </p>
              <div className="popover-actions">
                <button type="button" className="secondary-button" data-testid="auto-analysis-cancel" onClick={onCancel}>
                  {labels.cancel}
                </button>
                {status === "Paused" ? (
                  <button type="button" className="primary-button" data-testid="auto-analysis-resume" onClick={onResume}>
                    {labels.resume}
                  </button>
                ) : (
                  <button type="button" className="primary-button" data-testid="auto-analysis-pause" onClick={onPause}>
                    {labels.pause}
                  </button>
                )}
              </div>
            </>
          ) : (
            <>
              <p className="popover-text">{labels.scope.replace("{n}", String(positions))}</p>
              <div className="segmented" role="radiogroup" aria-label={labels.visitsPerMove.replace("{visits}", "")}>
                {PRESETS.map((preset) => (
                  <button
                    key={preset.key}
                    type="button"
                    role="radio"
                    aria-checked={visits === preset.visits}
                    data-testid={`auto-analysis-${preset.key}`}
                    onClick={() => setVisits(preset.visits)}
                  >
                    <span>{labels[preset.key]}</span>
                    <small>{preset.visits >= 1000 ? `${preset.visits / 1000}k` : preset.visits}</small>
                  </button>
                ))}
              </div>
              {outcome ? (
                <p className="popover-text" data-outcome={status}>
                  {outcome}
                </p>
              ) : null}
              <div className="popover-actions">
                <button
                  type="button"
                  className="primary-button"
                  data-testid="auto-analysis-start"
                  disabled={positions === 0}
                  onClick={() => {
                    onStart(visits);
                    setOpen(false);
                  }}
                >
                  {labels.start}
                </button>
              </div>
            </>
          )}
        </div>
      ) : null}
    </div>
  );
}
