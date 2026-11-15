import type { ScoringRuleView, ScoringView } from "@/api/backend";
import type en from "../locales/en.json";
import { resultText, scoreNumber, scoreTerms } from "./tools";

type Labels = typeof en.analysisTools.scoring;

export interface ScoringPanelProps {
  view: ScoringView;
  labels: Labels;
  onRule(rule: ScoringRuleView): void;
  onConfirm(): void;
  onCancel(): void;
}

/**
 * 点目结果, beside the board while 终局点目 is on: the rule, each side's sum term by term as Rust
 * counts it, and the result. 确定 writes the result into the game; 取消 leaves it as it was.
 */
export function ScoringPanel({ view, labels, onRule, onConfirm, onCancel }: ScoringPanelProps) {
  const side = (which: "black" | "white") => (
    <p className="scoring-line" data-testid={`scoring-${which}`}>
      <span className="scoring-side">{which === "black" ? labels.black : labels.white}</span>
      <span className="scoring-sum">
        {scoreTerms(view, which, labels.terms).map((term, index) => (
          <span key={term.label} className="scoring-term">
            {index > 0 ? <span className="scoring-op">+</span> : null}
            <span className="scoring-label">{term.label}</span> {scoreNumber(term.value)}
          </span>
        ))}
        <span className="scoring-op">=</span>
        <strong>{scoreNumber(view[which].total)}</strong>
      </span>
    </p>
  );
  const winner = view.result.startsWith("B") ? "Black" : view.result.startsWith("W") ? "White" : "None";
  return (
    <section className="scoring-panel" aria-label={labels.title} data-testid="scoring-panel">
      <header className="scoring-header">
        <h2>{labels.title}</h2>
        <div className="scoring-rules" role="group" aria-label={labels.rule}>
          {(["Area", "Territory"] as const).map((rule) => (
            <button key={rule} type="button" className="scoring-rule" aria-pressed={view.rule === rule} data-testid={`scoring-rule-${rule.toLowerCase()}`} onClick={() => view.rule !== rule && onRule(rule)}>
              {rule === "Area" ? labels.area : labels.territory}
            </button>
          ))}
        </div>
      </header>
      <p className="scoring-hint">{labels.hint}</p>
      {side("black")}
      {side("white")}
      <p className="scoring-result" data-winner={winner} data-testid="scoring-result">
        {resultText(view.result, labels)}
      </p>
      <footer className="scoring-buttons">
        <button type="button" className="secondary-button" data-testid="scoring-cancel" onClick={onCancel}>
          {labels.cancel}
        </button>
        <button type="button" className="primary-button" data-testid="scoring-confirm" onClick={onConfirm}>
          {labels.confirm}
        </button>
      </footer>
    </section>
  );
}
