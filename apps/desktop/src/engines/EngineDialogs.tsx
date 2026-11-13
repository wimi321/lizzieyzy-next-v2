import { useEffect, useState } from "react";
import type { EngineRules, JavaImportPreview } from "@/api/backend";
import { RULES } from "./model";

function useEscape(onClose: () => void) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
}

export interface RulesLabels {
  title: string;
  intro: string;
  /** A name for each rule set, by its type. */
  names: Readonly<Record<string, string>>;
  custom: string;
  save: string;
  cancel: string;
  close: string;
}

/** 设置 → 引擎规则 (Java's 实时修改引擎规则): the rules KataGo scores by, applied at once. */
export function RulesDialog({ rules, labels, onSave, onClose }: { rules: EngineRules; labels: RulesLabels; onSave(rules: EngineRules): void; onClose(): void }) {
  useEscape(onClose);
  const [chosen, setChosen] = useState<EngineRules>(rules);
  // A custom rule set from the Java import stays selectable as it is.
  const choices: EngineRules[] = [...RULES.map((type) => ({ type }) as EngineRules), ...(rules.type === "Custom" ? [rules] : [])];
  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog edit-dialog" role="dialog" aria-modal="true" aria-labelledby="rules-title" data-testid="rules-dialog">
        <header className="dialog-header">
          <h2 id="rules-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <p className="dialog-intro">{labels.intro}</p>
        <div className="rules-choices" role="radiogroup" aria-labelledby="rules-title">
          {choices.map((choice) => (
            <label key={choice.type} className="rules-choice">
              <input
                type="radio"
                name="engine-rules"
                data-testid={`rules-${choice.type}`}
                checked={chosen.type === choice.type}
                onChange={() => setChosen(choice)}
              />
              <span>{choice.type === "Custom" ? labels.custom : (labels.names[choice.type] ?? choice.type)}</span>
            </label>
          ))}
        </div>
        <footer className="dialog-footer">
          <button type="button" className="secondary-button" onClick={onClose}>
            {labels.cancel}
          </button>
          <button
            type="button"
            className="primary-button"
            data-testid="rules-save"
            disabled={JSON.stringify(chosen) === JSON.stringify(rules)}
            onClick={() => onSave(chosen)}
          >
            {labels.save}
          </button>
        </footer>
      </section>
    </div>
  );
}

export interface JavaImportLabels {
  title: string;
  intro: string;
  engines: string;
  main: string;
  noEngines: string;
  issues: string;
  confirm: string;
  cancel: string;
  close: string;
}

/** What importing the Java app's configuration would do; nothing is written until it is confirmed. */
export function JavaImportDialog({
  preview,
  busy,
  labels,
  onConfirm,
  onClose,
}: {
  preview: JavaImportPreview;
  busy: boolean;
  labels: JavaImportLabels;
  onConfirm(): void;
  onClose(): void;
}) {
  useEscape(onClose);
  const engines = preview.engines ?? [];
  const issues = preview.issues ?? [];
  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog engine-list-dialog" role="dialog" aria-modal="true" aria-labelledby="java-import-title" data-testid="java-import">
        <header className="dialog-header">
          <h2 id="java-import-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose} disabled={busy}>
            ×
          </button>
        </header>
        <p className="dialog-intro" title={preview.directory}>
          {labels.intro.replace("{dir}", preview.directory ?? "")}
        </p>
        <h3 className="dialog-subhead">{labels.engines}</h3>
        {engines.length === 0 ? <p className="engine-list-empty">{labels.noEngines}</p> : null}
        <ol className="engine-list">
          {engines.map((imported, index) => {
            const entry = imported.entry;
            const isMain = entry?.id !== undefined && entry.id === preview.imported_main_engine_id;
            return (
              <li key={entry?.id ?? index} className="engine-row" data-testid="java-import-engine" data-problem={(imported.issues?.length ?? 0) > 0 || undefined}>
                <span className="engine-index">{`[${index + 1}]`}</span>
                <span className="engine-name">
                  <strong>{entry?.name ?? ""}</strong>
                  <span className="engine-files" title={entry?.profile?.model}>
                    {entry?.profile ? `${entry.profile.program.split(/[\\/]/).pop()} · ${entry.profile.model.split(/[\\/]/).pop()}` : ""}
                  </span>
                  {(imported.issues ?? []).map((issue) => (
                    <span key={issue} className="engine-issue">
                      {issue}
                    </span>
                  ))}
                </span>
                {isMain ? <span className="engine-main-tag">{labels.main}</span> : null}
              </li>
            );
          })}
        </ol>
        {issues.length > 0 ? (
          <div className="dialog-error" role="alert" data-testid="java-import-issues">
            <strong>{labels.issues}</strong>
            <pre>{issues.join("\n")}</pre>
          </div>
        ) : null}
        <footer className="dialog-footer">
          <button type="button" className="secondary-button" onClick={onClose} disabled={busy}>
            {labels.cancel}
          </button>
          <button type="button" className="primary-button" data-testid="java-import-confirm" disabled={busy} onClick={onConfirm}>
            {labels.confirm}
          </button>
        </footer>
      </section>
    </div>
  );
}
