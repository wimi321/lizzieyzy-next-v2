import { type FormEvent, useEffect, useState } from "react";
import type { SearchParams } from "@/api/backend";
import type en from "../locales/en.json";
import { PARAMS, type ParamKey, paramOf, paramsOrNull } from "./tools";

type Labels = typeof en.analysisTools.params;

export interface SearchParamsDialogProps {
  params: SearchParams | null;
  labels: Labels;
  onSave(params: SearchParams | null): void;
  onClose(): void;
}

function textOf(params: SearchParams | null, key: ParamKey): string {
  const value = params?.[key];
  return value === null || value === undefined ? "" : String(value);
}

/**
 * 引擎参数 (Alt+D), Java's 设置KataGo引擎高级参数: PDA, WRN and the root policy temperature, each
 * left empty for KataGo's own value. They go with every query, so nothing restarts.
 */
export function SearchParamsDialog({ params, labels, onSave, onClose }: SearchParamsDialogProps) {
  const [texts, setTexts] = useState<Record<ParamKey, string>>(() => ({
    playout_doubling_advantage: textOf(params, "playout_doubling_advantage"),
    wide_root_noise: textOf(params, "wide_root_noise"),
    root_policy_temperature: textOf(params, "root_policy_temperature"),
  }));
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const parsed = PARAMS.map(({ key }) => [key, paramOf(texts[key], key)] as const);
  const valid = parsed.every(([, value]) => value !== undefined);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    onSave(paramsOrNull(Object.fromEntries(parsed) as SearchParams));
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog edit-dialog params-dialog" role="dialog" aria-modal="true" aria-labelledby="params-title" data-testid="params-dialog">
        <header className="dialog-header">
          <h2 id="params-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <form className="edit-form" noValidate onSubmit={submit}>
          {PARAMS.map(({ key, min, max }) => {
            const field = labels.fields[key];
            const bad = paramOf(texts[key], key) === undefined;
            return (
              <label key={key} className="params-field">
                <span className="params-name">
                  {field.name} <code>{field.setting}</code>
                </span>
                <span className="params-help">{field.help}</span>
                <span className="params-row">
                  <input
                    className="text-input"
                    inputMode="decimal"
                    value={texts[key]}
                    placeholder={labels.unset}
                    aria-invalid={bad}
                    data-testid={`params-${key}`}
                    onChange={(event) => setTexts({ ...texts, [key]: event.target.value })}
                  />
                  <span className={bad ? "params-range edit-invalid" : "params-range"}>{labels.range.replace("{min}", String(min)).replace("{max}", String(max))}</span>
                </span>
              </label>
            );
          })}
          <p className="params-hint">{labels.hint}</p>
          <footer className="dialog-footer">
            <button
              type="button"
              className="secondary-button params-reset"
              data-testid="params-reset"
              onClick={() => setTexts({ playout_doubling_advantage: "", wide_root_noise: "", root_policy_temperature: "" })}
            >
              {labels.reset}
            </button>
            <button type="button" className="secondary-button" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" disabled={!valid} data-testid="params-ok">
              {labels.confirm}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
