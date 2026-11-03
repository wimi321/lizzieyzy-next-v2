import { useEffect, useState } from "react";
import { ToolButton } from "../frame/ToolButton";

export interface LimitDialogLabels {
  title: string;
  line1: string;
  line2: string;
  noAgain: string;
  ok: string;
}

/**
 * The message LizzieYzy Next shows when a move's analysis stops at its time or visit limit
 * (`Leelaz.showStopPonderTips`): a small non-modal box that leaves the board usable, so Space
 * still resumes. "Do not show again" holds for this run.
 */
export function LimitDialog({ labels, onClose }: { labels: LimitDialogLabels; onClose(noAgain: boolean): void }) {
  const [noAgain, setNoAgain] = useState(false);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose(noAgain);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [noAgain, onClose]);
  return (
    <section className="limit-dialog" role="dialog" aria-modal="false" aria-labelledby="limit-dialog-title" data-testid="limit-dialog">
      <h2 id="limit-dialog-title">{labels.title}</h2>
      <p>{labels.line1}</p>
      <p>{labels.line2}</p>
      <label className="jt-check">
        <input type="checkbox" checked={noAgain} data-testid="limit-no-again" onChange={(event) => setNoAgain(event.target.checked)} />
        <span>{labels.noAgain}</span>
      </label>
      <footer>
        <ToolButton label={labels.ok} testId="limit-ok" onClick={() => onClose(noAgain)} />
      </footer>
    </section>
  );
}
