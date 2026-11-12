import type { ReactNode } from "react";

export interface NotesLabels {
  comment: string;
  problems: string;
  noComment: string;
}

export interface ReviewStatus {
  state: "running" | "partial" | "done";
  text: string;
}

export interface NotesPanelProps {
  tab: "comment" | "problems";
  onTab(tab: "comment" | "problems"): void;
  problemCount: number;
  /** How far the whole game is evaluated, at the right of the tabs. */
  status: ReviewStatus | null;
  /** The analysis note on the current move, one entry per line; empty before any analysis. */
  summary: readonly string[];
  /** The comment recorded in the game for the current move. */
  comment: string | null;
  labels: NotesLabels;
  /** The problem-move list, shown on its tab. */
  children: ReactNode;
}

/**
 * LizzieYzy Next's sidebar under the graph: 评论 and 问题手 tabs with the evaluation state beside
 * them. The comment tab opens with the analysis note on the move, then the game's own comment.
 */
export function NotesPanel({ tab, onTab, problemCount, status, summary, comment, labels, children }: NotesPanelProps) {
  return (
    <section className="notes">
      <header className="notes-head">
        <div className="notes-tabs" role="tablist">
          <button type="button" role="tab" aria-selected={tab === "comment"} data-testid="tab-comment" onClick={() => onTab("comment")}>
            {labels.comment}
          </button>
          <button type="button" role="tab" aria-selected={tab === "problems"} data-testid="tab-problems" onClick={() => onTab("problems")}>
            {labels.problems}
            {problemCount > 0 ? <span className="tab-count">{problemCount}</span> : null}
          </button>
        </div>
        {status ? (
          <span className="notes-status" data-state={status.state} data-testid="review-status">
            <i aria-hidden="true" />
            {status.text}
          </span>
        ) : null}
      </header>
      {tab === "problems" ? (
        children
      ) : (
        <div className="notes-comment" data-testid="comment">
          {summary.length > 0 ? (
            <p className="notes-summary" data-testid="analysis-summary">
              {summary.join("\n")}
            </p>
          ) : null}
          {comment ? <p className="notes-text">{comment}</p> : summary.length === 0 ? <p className="panel-empty">{labels.noComment}</p> : null}
        </div>
      )}
    </section>
  );
}
