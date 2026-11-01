import type { BenchmarkStatus } from "@/api/backend";

export interface BenchmarkChipLabels {
  running: string;
  threadCount: string;
  stop: string;
  open: string;
}

/** In the menu strip's tray while KataGo is being tuned, so the window can close meanwhile. */
export function BenchmarkChip({ status, labels, onOpen, onStop }: { status: BenchmarkStatus; labels: BenchmarkChipLabels; onOpen(): void; onStop(): void }) {
  if (status.phase !== "Running") return null;
  const progress = status.progress;
  return (
    <span className="chip benchmark-chip" data-testid="benchmark-chip">
      <span className="sync-dot" aria-hidden="true" />
      <button type="button" className="benchmark-chip-open" title={labels.open} onClick={onOpen}>
        <span className="chip-label">{labels.running}</span>
        {progress ? (
          <strong>
            {labels.threadCount.replace("{n}", String(progress.threads))} · {progress.completed_positions}/{progress.total_positions}
          </strong>
        ) : null}
      </button>
      <button type="button" className="sync-stop" data-testid="benchmark-chip-stop" onClick={onStop}>
        {labels.stop}
      </button>
    </span>
  );
}
