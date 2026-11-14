import { type FormEvent, useState } from "react";
import type { CommentaryUpdated } from "../api/commentary";
import type en from "../locales/en.json";
import { isWorking, statusText } from "./format";
import { Markdown } from "./Markdown";

export type CommentaryModeChoice = "next" | "range" | "whole";

/** One answer of the conversation: the first one, then each follow-up with its question. */
export interface CommentaryEntry {
  id: number;
  question: string | null;
  view: CommentaryUpdated;
}

export interface CommentaryWindowProps {
  labels: typeof en.commentary;
  mode: CommentaryModeChoice;
  onMode(mode: CommentaryModeChoice): void;
  /** The current move number, or null without a game. */
  current: number | null;
  range: { from: string; to: string };
  onRange(range: { from: string; to: string }): void;
  configured: boolean;
  entries: readonly CommentaryEntry[];
  /** A request the backend refused, already in words. */
  failure: string | null;
  model: string;
  writeToComment: boolean;
  onWriteToComment(on: boolean): void;
  onStart(): void;
  onStop(): void;
  onAsk(question: string): void;
  onSettings(): void;
  onJump(node: number): void;
  onClose(): void;
}

/** Java's TeacherDialog: mode and settings, the position, the answer as it arrives, follow-ups. */
export function CommentaryWindow(props: CommentaryWindowProps) {
  const { labels, mode, current, range, entries, failure } = props;
  const [tab, setTab] = useState<"lesson" | "evidence">("lesson");
  const [question, setQuestion] = useState("");
  const last = entries[entries.length - 1];
  const busy = isWorking(last?.view);
  const canAsk = !busy && props.configured && last !== undefined && (last.view.text ?? "") !== "";

  const ask = (text: string) => {
    if (!canAsk || text.trim() === "") return;
    props.onAsk(text.trim());
    setQuestion("");
    setTab("lesson");
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    ask(question);
  };
  const start = () => {
    setTab("lesson");
    if (props.configured) props.onStart();
    else props.onSettings();
  };
  const positions = last?.view.positions ?? [];
  const issues = last?.view.issues ?? [];

  return (
    <section className="dialog commentary-window" role="dialog" aria-modal="false" aria-labelledby="commentary-title" data-testid="commentary-window">
      <header className="dialog-header commentary-head">
        <h2 id="commentary-title">{labels.title}</h2>
        <div className="segmented small" role="radiogroup" aria-labelledby="commentary-title">
          {(["next", "range", "whole"] as const).map((value) => (
            <button key={value} type="button" role="radio" aria-checked={mode === value} data-testid={`commentary-mode-${value}`} disabled={busy} onClick={() => props.onMode(value)}>
              {value === "next" ? labels.modeNext : value === "range" ? labels.modeRange : labels.modeWhole}
            </button>
          ))}
        </div>
        <button type="button" className="secondary-button small" data-testid="commentary-settings" disabled={busy} onClick={props.onSettings}>
          {labels.settings}
        </button>
        <button type="button" className="dialog-close" aria-label={labels.close} data-testid="commentary-close" onClick={props.onClose}>
          ×
        </button>
      </header>
      <div className="commentary-context">
        {mode === "next" ? (
          <span data-testid="commentary-position">{current === null ? labels.positionNone : labels.positionMove.replace("{n}", String(current))}</span>
        ) : mode === "range" ? (
          <span className="commentary-range">
            <label>
              {labels.rangeFrom}
              <input className="text-input" inputMode="numeric" data-testid="commentary-from" disabled={busy} value={range.from} onChange={(event) => props.onRange({ ...range, from: event.target.value })} />
            </label>
            <label>
              {labels.rangeTo}
              <input className="text-input" inputMode="numeric" data-testid="commentary-to" disabled={busy} value={range.to} onChange={(event) => props.onRange({ ...range, to: event.target.value })} />
            </label>
          </span>
        ) : (
          <span>{labels.positionWhole}</span>
        )}
        {mode !== "next" && positions.length > 0 ? (
          <span className="commentary-positions">
            {labels.keyPositions}
            {positions.map((position) => (
              <button key={position.node_id} type="button" className="tag" data-testid="commentary-key-position" onClick={() => position.node_id !== undefined && props.onJump(position.node_id)}>
                {labels.movePosition.replace("{n}", String(position.move_number ?? 0))}
              </button>
            ))}
          </span>
        ) : null}
        {busy ? (
          <button type="button" className="secondary-button" data-testid="commentary-stop" onClick={props.onStop}>
            {labels.stop}
          </button>
        ) : (
          <button type="button" className="primary-button" data-testid="commentary-start" onClick={start}>
            {props.configured ? labels.start : labels.connect}
          </button>
        )}
      </div>
      <div className="notes-tabs commentary-tabs" role="tablist">
        <button type="button" role="tab" aria-selected={tab === "lesson"} data-testid="commentary-tab-lesson" onClick={() => setTab("lesson")}>
          {labels.lesson}
        </button>
        <button type="button" role="tab" aria-selected={tab === "evidence"} data-testid="commentary-tab-evidence" onClick={() => setTab("evidence")}>
          {labels.evidence}
        </button>
      </div>
      <div className="commentary-body" aria-live="polite" data-testid="commentary-body">
        {tab === "evidence" ? (
          <pre className="commentary-evidence" data-testid="commentary-evidence">
            {last?.view.evidence || labels.noEvidence}
          </pre>
        ) : entries.length === 0 ? (
          <div className="fox-state">
            <p className="commentary-empty-title">{labels.emptyTitle}</p>
            <p>{props.configured ? labels.emptyGuide : labels.emptyConnect}</p>
          </div>
        ) : (
          entries.map((entry) => (
            <article key={entry.id} className="commentary-answer" data-testid="commentary-answer">
              {entry.question !== null ? <h3 className="commentary-question">{labels.asked.replace("{question}", entry.question)}</h3> : null}
              {entry.view.text ? <Markdown text={entry.view.text} /> : <p className="panel-empty">{isWorking(entry.view) ? labels.phases.Preparing : ""}</p>}
            </article>
          ))
        )}
      </div>
      <div className="commentary-status" data-testid="commentary-status" data-phase={failure ? "Failed" : (last?.view.phase ?? "Stopped")}>
        <i aria-hidden="true" />
        <span className="commentary-status-text">{failure ?? (last ? statusText(last.view, props.writeToComment, labels) : labels.idle)}</span>
        {props.model ? <span className="commentary-model">{labels.model.replace("{model}", props.model)}</span> : null}
      </div>
      {issues.length > 0 ? (
        <ul className="commentary-issues" data-testid="commentary-issues">
          {issues.map((issue, index) => (
            <li key={index}>{issue}</li>
          ))}
        </ul>
      ) : null}
      <form className="commentary-ask" onSubmit={submit}>
        <input
          className="text-input"
          data-testid="commentary-question"
          aria-label={labels.followUp}
          placeholder={labels.followUpPlaceholder}
          disabled={!canAsk}
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
        />
        <button type="submit" className="secondary-button" data-testid="commentary-ask" disabled={!canAsk || question.trim() === ""}>
          {labels.ask}
        </button>
      </form>
      <footer className="commentary-footer">
        <button type="button" className="tag" data-testid="commentary-simpler" disabled={!canAsk} onClick={() => ask(labels.simpler)}>
          {labels.simpler}
        </button>
        <button type="button" className="tag" data-testid="commentary-why" disabled={!canAsk} onClick={() => ask(labels.whyReply)}>
          {labels.whyReply}
        </button>
        <label className="commentary-write">
          <input type="checkbox" data-testid="commentary-write" checked={props.writeToComment} onChange={(event) => props.onWriteToComment(event.target.checked)} />
          {labels.writeToComment}
        </label>
      </footer>
    </section>
  );
}
