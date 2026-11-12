import { type FormEvent, useEffect, useState } from "react";
import type { GameInfoEdit, GameInfoView } from "@/api/backend";
import { editedText, RULE_PRESETS } from "./info";

export interface GameInfoLabels {
  infoTitle: string;
  blackName: string;
  blackRank: string;
  whiteName: string;
  whiteRank: string;
  komi: string;
  rules: string;
  result: string;
  date: string;
  event: string;
  komiInvalid: string;
  failed: string;
  confirm: string;
  cancel: string;
  close: string;
}

export interface GameInfoDialogProps {
  info: GameInfoView;
  labels: GameInfoLabels;
  /** The six rule presets by their canonical names, in the interface language. */
  ruleNames: Readonly<Record<string, string>>;
  /** Applies the edit; rejects with the reason when it cannot. */
  onSave(edit: GameInfoEdit): Promise<void>;
  onClose(): void;
}

type TextField = "black_name" | "black_rank" | "white_name" | "white_rank" | "result" | "date" | "event";
const TEXT_FIELDS: readonly TextField[] = ["black_name", "black_rank", "white_name", "white_rank", "result", "date", "event"];

function describe(error: unknown): string {
  if (typeof error === "object" && error !== null && "data" in error && typeof error.data === "string") return error.data;
  return error instanceof Error ? error.message : String(error);
}

/** 编辑 → 编辑棋局信息 (`GameInfoDialog`): the players, the komi and what else the game's root records. */
export function GameInfoDialog({ info, labels, ruleNames, onSave, onClose }: GameInfoDialogProps) {
  const [text, setText] = useState<Record<TextField, string>>(
    () => Object.fromEntries(TEXT_FIELDS.map((field) => [field, info[field] ?? ""])) as Record<TextField, string>,
  );
  const [komi, setKomi] = useState(String(info.komi));
  const [rules, setRules] = useState(info.rules_preset ?? "chinese");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const komiValue = komi.trim() === "" ? Number.NaN : Number(komi.trim());
  const valid = Number.isFinite(komiValue);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    setBusy(true);
    setError(null);
    onSave({
      black_name: editedText(info.black_name, text.black_name),
      black_rank: editedText(info.black_rank, text.black_rank),
      white_name: editedText(info.white_name, text.white_name),
      white_rank: editedText(info.white_rank, text.white_rank),
      komi: komiValue,
      rules,
      result: editedText(info.result, text.result),
      date: editedText(info.date, text.date),
      event: editedText(info.event, text.event),
    })
      .catch((reason: unknown) => setError(describe(reason)))
      .finally(() => setBusy(false));
  };

  const field = (name: TextField, label: string, wide = false) => (
    <label className={wide ? "edit-field wide" : "edit-field"}>
      <span>{label}</span>
      <input
        className="text-input"
        data-testid={`game-info-${name}`}
        value={text[name]}
        onChange={(event) => setText((current) => ({ ...current, [name]: event.target.value }))}
      />
    </label>
  );

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog edit-dialog" role="dialog" aria-modal="true" aria-labelledby="game-info-title" data-testid="game-info-dialog">
        <header className="dialog-header">
          <h2 id="game-info-title">{labels.infoTitle}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <form className="edit-form" noValidate onSubmit={submit}>
          <div className="edit-grid">
            {field("black_name", labels.blackName)}
            {field("black_rank", labels.blackRank)}
            {field("white_name", labels.whiteName)}
            {field("white_rank", labels.whiteRank)}
            <label className="edit-field">
              <span>{labels.komi}</span>
              <input
                className="text-input"
                inputMode="decimal"
                data-testid="game-info-komi"
                aria-invalid={!valid}
                value={komi}
                onChange={(event) => setKomi(event.target.value)}
              />
            </label>
            <label className="edit-field">
              <span>{labels.rules}</span>
              <select className="text-input" data-testid="game-info-rules" value={rules} onChange={(event) => setRules(event.target.value)}>
                {RULE_PRESETS.map((preset) => (
                  <option key={preset} value={preset}>
                    {ruleNames[preset] ?? preset}
                  </option>
                ))}
              </select>
            </label>
            {field("result", labels.result)}
            {field("date", labels.date)}
            {field("event", labels.event, true)}
          </div>
          {!valid ? <p className="edit-invalid">{labels.komiInvalid}</p> : null}
          {error ? (
            <div className="dialog-error" role="alert" data-testid="game-info-error">
              <strong>{labels.failed}</strong>
              <pre>{error}</pre>
            </div>
          ) : null}
          <footer className="dialog-footer">
            <button type="button" className="secondary-button" data-testid="game-info-cancel" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="game-info-confirm" disabled={!valid || busy}>
              {labels.confirm}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
