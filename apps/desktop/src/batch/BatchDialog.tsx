import { type FormEvent, useEffect, useState } from "react";
import type { WholeGameScope } from "@/api/backend";
import type { BatchLabels, PresetLabels } from "./labels";
import { rangeScope, wholeNumber } from "./range";

/** Visits per move, as the whole-game presets. */
const PRESETS = [
  { key: "quick", visits: 500 },
  { key: "standard", visits: 1000 },
  { key: "deep", visits: 3000 },
  { key: "professional", visits: 10_000 },
] as const;

export interface BatchChoice {
  visits: number;
  scope: WholeGameScope;
  force: boolean;
  directory: string | null;
}

export interface BatchDialogProps {
  /** 批量精析 takes visits and overwriting from 精析设置; 批量分析 chooses them here. */
  flash: { visits: number; force: boolean } | null;
  /** Games chosen just now, and games already waiting in the queue. */
  files: readonly string[];
  waiting: number;
  labels: BatchLabels;
  presets: PresetLabels;
  pickFolder(): Promise<string | null>;
  onStart(choice: BatchChoice): void;
  onClose(): void;
}

const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** Java's 自动分析设置 for a batch: how deep, which part of each game, and where the results go. */
export function BatchDialog({ flash, files, waiting, labels, presets, pickFolder, onStart, onClose }: BatchDialogProps) {
  const [choice, setChoice] = useState<number | "custom">(1000);
  const [custom, setCustom] = useState("");
  const [scope, setScope] = useState<"line" | "range" | "branches">("line");
  const [first, setFirst] = useState("");
  const [last, setLast] = useState("");
  const [beside, setBeside] = useState(true);
  const [folder, setFolder] = useState<string | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const customVisits = wholeNumber(custom);
  const visits = flash ? flash.visits : choice === "custom" ? (customVisits !== null && customVisits >= 500 && customVisits <= 1_000_000 ? customVisits : null) : choice;
  const chosenScope: WholeGameScope | null = scope === "line" ? { type: "Line" } : scope === "branches" ? { type: "AllBranches" } : rangeScope(first, last);
  const directory = beside ? null : folder;
  const ready = visits !== null && chosenScope !== null && (beside || folder !== null) && files.length + waiting > 0;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!ready || visits === null || chosenScope === null) return;
    onStart({ visits, scope: chosenScope, force: flash?.force ?? false, directory });
  };

  const names = files.map(baseName);
  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog batch-dialog" role="dialog" aria-modal="true" aria-labelledby="batch-title" data-testid="batch-dialog">
        <header className="dialog-header">
          <h2 id="batch-title">{flash ? labels.flashTitle : labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <form className="batch-form" noValidate onSubmit={submit}>
          <p className="batch-files" data-testid="batch-files" title={names.join("\n")}>
            {files.length > 0 ? labels.files.replace("{n}", String(files.length)) : null}
            {files.length > 0 ? <span>{names.slice(0, 3).join("、") + (names.length > 3 ? " …" : "")}</span> : null}
            {waiting > 0 ? <span className="batch-waiting">{labels.queued.replace("{n}", String(waiting))}</span> : null}
          </p>

          <h3 className="dialog-subhead">{labels.visits}</h3>
          {flash ? (
            <p className="settings-hint batch-flash-visits" data-testid="batch-flash-visits">
              {labels.flashVisits.replace("{visits}", String(flash.visits))}
              {flash.force ? ` · ${labels.override}` : ""}
            </p>
          ) : (
            <>
              <div className="segmented" role="radiogroup" aria-label={labels.visits}>
                {PRESETS.map((preset) => (
                  <button
                    key={preset.key}
                    type="button"
                    role="radio"
                    aria-checked={choice === preset.visits}
                    data-testid={`batch-${preset.key}`}
                    onClick={() => setChoice(preset.visits)}
                  >
                    <span>{presets[preset.key]}</span>
                    <small>{preset.visits >= 1000 ? `${preset.visits / 1000}k` : preset.visits}</small>
                  </button>
                ))}
              </div>
              <label className={choice === "custom" ? "custom-visits active" : "custom-visits"}>
                <span>{presets.custom}</span>
                <input
                  type="text"
                  inputMode="numeric"
                  className="text-input"
                  data-testid="batch-custom"
                  value={custom}
                  placeholder="2000"
                  aria-invalid={choice === "custom" && visits === null}
                  onFocus={() => setChoice("custom")}
                  onChange={(event) => {
                    setCustom(event.target.value);
                    setChoice("custom");
                  }}
                />
                <span className="custom-unit">{presets.perMove}</span>
              </label>
              {choice === "custom" && visits === null && custom.trim() !== "" ? <p className="settings-hint batch-invalid">{presets.customRange}</p> : null}
            </>
          )}

          <h3 className="dialog-subhead">{labels.scope}</h3>
          <div className="batch-options" role="radiogroup" aria-label={labels.scope}>
            <label className="settings-check">
              <input type="radio" name="batch-scope" data-testid="batch-scope-line" checked={scope === "line"} onChange={() => setScope("line")} />
              <span>{labels.scopeLine}</span>
            </label>
            <label className="settings-check batch-range">
              <input type="radio" name="batch-scope" data-testid="batch-scope-range" checked={scope === "range"} onChange={() => setScope("range")} />
              <span>{labels.scopeRange}</span>
              <input
                className="text-input"
                inputMode="numeric"
                data-testid="batch-first"
                aria-label={labels.startMove}
                placeholder={labels.startMove}
                aria-invalid={scope === "range" && chosenScope === null}
                value={first}
                onFocus={() => setScope("range")}
                onChange={(event) => setFirst(event.target.value)}
              />
              <span>–</span>
              <input
                className="text-input"
                inputMode="numeric"
                data-testid="batch-last"
                aria-label={labels.endMove}
                placeholder={labels.endMove}
                aria-invalid={scope === "range" && chosenScope === null}
                value={last}
                onFocus={() => setScope("range")}
                onChange={(event) => setLast(event.target.value)}
              />
            </label>
            <label className="settings-check">
              <input type="radio" name="batch-scope" data-testid="batch-scope-branches" checked={scope === "branches"} onChange={() => setScope("branches")} />
              <span>{labels.scopeBranches}</span>
            </label>
            {scope === "range" ? <p className="settings-hint">{chosenScope === null ? labels.rangeInvalid : labels.rangeNotice}</p> : null}
          </div>

          <h3 className="dialog-subhead">{labels.output}</h3>
          <div className="batch-options" role="radiogroup" aria-label={labels.output}>
            <label className="settings-check">
              <input type="radio" name="batch-output" data-testid="batch-output-beside" checked={beside} onChange={() => setBeside(true)} />
              <span>{labels.outputBeside}</span>
            </label>
            <label className="settings-check batch-folder">
              <input type="radio" name="batch-output" data-testid="batch-output-folder" checked={!beside} onChange={() => setBeside(false)} />
              <span>{labels.outputFolder}</span>
              <span className="batch-folder-path" data-testid="batch-folder" title={folder ?? undefined}>
                {folder ?? labels.noFolder}
              </span>
              <button
                type="button"
                className="secondary-button"
                data-testid="batch-choose-folder"
                onClick={() =>
                  void pickFolder().then((picked) => {
                    if (picked) {
                      setFolder(picked);
                      setBeside(false);
                    }
                  })
                }
              >
                {labels.chooseFolder}
              </button>
            </label>
            <p className="settings-hint">{labels.outputHint.replace("{suffix}", labels.suffix)}</p>
          </div>

          <footer className="dialog-footer settings-footer">
            <button type="button" className="secondary-button" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="batch-start" disabled={!ready}>
              {labels.start}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
