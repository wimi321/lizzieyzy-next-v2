import type { ReactNode } from "react";
import { shortGeneratedAt, splitComment } from "../commentary/block";
import { Markdown } from "../commentary/Markdown";

export interface NotesLabels {
  comment: string;
  problems: string;
  noComment: string;
  editHint: string;
  aiCommentary: string;
  aiFollowUp: string;
}

/** The comment while it is being edited: Java swaps the text for an editor in place. */
export interface CommentEditor {
  text: string;
  onChange(text: string): void;
  /** Leaving the editor saves; Ctrl+Enter does too. */
  onCommit(): void;
  /** Escape puts the comment back as it was. */
  onCancel(): void;
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
  /** Set while the comment is being edited. */
  editor?: CommentEditor | null;
  /** A click on the comment starts editing it; absent when there is no game to edit. */
  onEdit?(): void;
  labels: NotesLabels;
  /** The problem-move list, shown on its tab. */
  children: ReactNode;
}

/**
 * LizzieYzy Next's sidebar under the graph: 评论 and 问题手 tabs with the evaluation state beside
 * them. The comment tab opens with the analysis note on the move, then the game's own comment.
 */
export function NotesPanel({ tab, onTab, problemCount, status, summary, comment, editor = null, onEdit, labels, children }: NotesPanelProps) {
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
          {editor ? (
            <textarea
              className="notes-editor"
              data-testid="comment-editor"
              aria-label={labels.comment}
              autoFocus
              value={editor.text}
              onChange={(event) => editor.onChange(event.target.value)}
              onBlur={editor.onCommit}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  event.preventDefault();
                  editor.onCancel();
                } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                  event.preventDefault();
                  editor.onCommit();
                }
              }}
            />
          ) : (
            <div className="notes-text" data-testid="comment-text" data-editable={onEdit ? "" : undefined} title={onEdit ? labels.editHint : undefined} onClick={onEdit}>
              {comment ? <CommentText comment={comment} labels={labels} /> : <p className="panel-empty">{labels.noComment}</p>}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

/** The user's own comment, then the AI 解说 block Rust wrote, with its follow-ups after it. */
function CommentText({ comment, labels }: { comment: string; labels: NotesLabels }) {
  const { own, ai } = splitComment(comment);
  return (
    <>
      {own ? <p>{own}</p> : null}
      {ai ? (
        <section className="notes-ai" data-testid="comment-ai">
          <h3>
            {labels.aiCommentary}
            <span className="notes-ai-meta">{[ai.model, shortGeneratedAt(ai.generatedAt)].filter(Boolean).join(" · ")}</span>
          </h3>
          {ai.sections.map((section, index) => (
            <div key={index} className="notes-ai-section">
              {section.question !== null ? <h4>{labels.aiFollowUp.replace("{question}", section.question)}</h4> : null}
              <Markdown text={section.body} />
            </div>
          ))}
        </section>
      ) : null}
    </>
  );
}
