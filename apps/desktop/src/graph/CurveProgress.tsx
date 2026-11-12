export interface CurveProgressLabels {
  /** "正在生成 {done}/{total}" */
  quick: string;
  quickStop: string;
}

/** The quiet one-visit pass that draws the graph when a game opens: how far it is, and a way to stop it. */
export function CurveProgress({ done, total, labels, onStop }: { done: number; total: number; labels: CurveProgressLabels; onStop(): void }) {
  return (
    <span className="curve-progress" data-testid="curve-progress">
      <span className="curve-progress-bar" aria-hidden="true">
        <span style={{ width: `${(done / Math.max(1, total)) * 100}%` }} />
      </span>
      {labels.quick.replace("{done}", String(done)).replace("{total}", String(total))}
      <button type="button" className="curve-progress-stop" data-testid="curve-progress-stop" title={labels.quickStop} aria-label={labels.quickStop} onClick={onStop}>
        ×
      </button>
    </span>
  );
}
