import { type FormEvent, useState } from "react";
import type { GtpEngineEntry } from "../api/gtp";
import type { MatchSetup } from "../api/match-game";
import { RULE_PRESETS } from "../edit/info";
import { AiFields, type EngineChoice, NumberField } from "./AiFields";
import type { MatchLabels, PlayLabels, RuleLabels } from "./labels";
import { PlayWindow, Segmented } from "./PlayDialog";
import { decimal, MATCH_FORM, type MatchForm, matchSetup, type MatchSideForm, matchSideForm, whole } from "./setup";

export interface KataGoEntry {
  id: string;
  name: string;
}

interface MatchDialogProps {
  katago: readonly KataGoEntry[];
  gtpEngines: readonly GtpEngineEntry[];
  canContinue: boolean;
  labels: MatchLabels;
  play: PlayLabels;
  rules: RuleLabels;
  pickOpenings(): Promise<string[]>;
  onStart(setup: MatchSetup): Promise<string | null>;
  onClose(): void;
}

const baseName = (path: string) => path.split(/[\\/]/).at(-1) ?? path;

/** 棋局 → 新对局 → 引擎对局: two engines, each with its own limits, over a number of games. */
export function MatchDialog({ katago, gtpEngines, canContinue, labels, play, rules, pickOpenings, onStart, onClose }: MatchDialogProps) {
  const choices: EngineChoice[] = [
    ...katago.map((entry) => ({ value: `k:${entry.id}`, label: `${entry.name} · KataGo`, gtp: false })),
    ...gtpEngines.map((entry) => ({ value: `g:${entry.id ?? ""}`, label: `${entry.name ?? entry.id} · GTP`, gtp: true })),
  ];
  const first = choices[0]?.value ?? "";
  const [sides, setSides] = useState<[MatchSideForm, MatchSideForm]>(() => [matchSideForm(first), matchSideForm(choices[1]?.value ?? first)]);
  const [form, setForm] = useState<MatchForm>(MATCH_FORM);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<MatchForm>) => setForm((current) => ({ ...current, ...patch }));
  const setSide = (index: 0 | 1, side: MatchSideForm) => setSides((current) => (index === 0 ? [side, current[1]] : [current[0], side]));
  const setup = matchSetup(form, sides[0], sides[1]);
  const ready = setup !== null && !busy;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!setup || busy) return;
    setBusy(true);
    setError(null);
    void onStart(setup).then((failure) => {
      setBusy(false);
      if (failure) setError(failure);
    });
  };

  const sidePanel = (index: 0 | 1) => {
    const side = sides[index];
    const gtp = side.ai.engine.startsWith("g:");
    return (
      <div className="play-panel" key={index}>
        <h3 className="dialog-subhead">{labels.side.replace("{n}", String(index + 1))}</h3>
        <AiFields
          form={side.ai}
          onChange={(ai) => setSide(index, { ...side, ai })}
          choices={choices}
          labels={play}
          engineLabel={labels.engine}
          more={!gtp}
          random={false}
          testId={`match-side-${index + 1}`}
          extra={
            <>
              <div className="play-row">
                <span className="play-label">{labels.wrn}</span>
                <NumberField value={side.wrn} valid={decimal(side.wrn, 0, 5) !== null} onChange={(wrn) => setSide(index, { ...side, wrn })} unit="0 – 5" testId={`match-side-${index + 1}-wrn`} />
              </div>
              <div className="play-row">
                <span className="play-label">{labels.pda}</span>
                <NumberField value={side.pda} valid={decimal(side.pda, -3, 3) !== null} onChange={(pda) => setSide(index, { ...side, pda })} unit="−3 – 3" testId={`match-side-${index + 1}-pda`} />
              </div>
            </>
          }
        />
      </div>
    );
  };

  return (
    <PlayWindow title={labels.title} testId="match-dialog" className="match-dialog" onClose={onClose} close={labels.close}>
      <form className="play-form" noValidate onSubmit={submit}>
        {choices.length === 0 ? <p className="play-error">{labels.chooseEngines}</p> : null}
        <div className="match-sides">
          {sidePanel(0)}
          {sidePanel(1)}
        </div>
        <div className="play-panel">
          <div className="play-row">
            <span className="play-label">{labels.games}</span>
            <span className="play-inline">
              <NumberField value={form.games} valid={whole(form.games, 1, 10_000) !== null} onChange={(games) => set({ games })} testId="match-games" />
              <label className="settings-check">
                <input type="checkbox" checked={form.swap} data-testid="match-swap" onChange={(event) => set({ swap: event.target.checked })} />
                <span>{labels.swap}</span>
              </label>
              <span className="settings-hint play-note">{labels.firstBlack}</span>
            </span>
          </div>
          <div className="play-row">
            <span className="play-label">{play.startFrom}</span>
            <Segmented
              value={form.fromCurrent ? "here" : "new"}
              label={play.startFrom}
              testId="match-start"
              onChange={(value) => set({ fromCurrent: value === "here" })}
              options={[
                { value: "new", label: play.newBoard },
                { value: "here", label: labels.fromCurrent, disabled: !canContinue },
              ]}
            />
          </div>
          {form.fromCurrent ? null : (
            <>
              <div className="play-row">
                <span className="play-label">{play.size}</span>
                <span className="play-inline">
                  <Segmented value={form.size} label={play.size} testId="match-size" onChange={(size) => set({ size })} options={[19, 13, 9].map((size) => ({ value: size, label: `${size}×${size}` }))} />
                  <span className="play-label play-gap">{play.handicap}</span>
                  <select className="select" value={form.handicap} data-testid="match-handicap" onChange={(event) => set({ handicap: Number(event.target.value) })}>
                    {[0, 2, 3, 4, 5, 6, 7, 8, 9].map((count) => (
                      <option key={count} value={count}>
                        {count === 0 ? play.noHandicap : count}
                      </option>
                    ))}
                  </select>
                </span>
              </div>
              <div className="play-row">
                <span className="play-label">{play.komi}</span>
                <span className="play-inline">
                  <NumberField value={form.komi} valid={decimal(form.komi, -1000) !== null} onChange={(komi) => set({ komi })} testId="match-komi" />
                  <span className="play-label play-gap">{play.rules}</span>
                  <select className="select" value={form.rules} data-testid="match-rules" onChange={(event) => set({ rules: event.target.value })}>
                    {RULE_PRESETS.map((preset) => (
                      <option key={preset} value={preset}>
                        {rules[preset]}
                      </option>
                    ))}
                  </select>
                </span>
              </div>
            </>
          )}
          <div className="play-row">
            <label className="settings-check play-label">
              <input type="checkbox" checked={form.limitMoves} data-testid="match-limit-moves" onChange={(event) => set({ limitMoves: event.target.checked })} />
              <span>{labels.maxMoves}</span>
            </label>
            <span className="play-inline">
              <NumberField value={form.maxMoves} valid={whole(form.maxMoves, 1) !== null} disabled={!form.limitMoves} onChange={(maxMoves) => set({ maxMoves })} testId="match-max-moves" />
              <span className="settings-hint play-note">{labels.maxMovesHint}</span>
            </span>
          </div>
          <div className="play-row">
            <span className="play-label">{labels.openings}</span>
            <span className="play-inline">
              <button type="button" className="secondary-button" data-testid="match-add-openings" onClick={() => void pickOpenings().then((paths) => set({ openings: [...new Set([...form.openings, ...paths])] }))}>
                {labels.addOpenings}
              </button>
              <button type="button" className="secondary-button" disabled={form.openings.length === 0} onClick={() => set({ openings: [] })}>
                {labels.clearOpenings}
              </button>
              <Segmented value={form.randomOrder ? "random" : "order"} label={labels.openings} testId="match-order" onChange={(value) => set({ randomOrder: value === "random" })} options={[{ value: "order", label: labels.inOrder }, { value: "random", label: labels.randomOrder }]} />
            </span>
          </div>
          <p className="settings-hint play-files" data-testid="match-openings" title={form.openings.join("\n")}>
            {form.openings.length === 0 ? labels.noOpenings : form.openings.map(baseName).join("、")}
          </p>
          <div className="play-row">
            <label className="settings-check play-label">
              <input type="checkbox" checked={form.random} data-testid="match-random" onChange={(event) => set({ random: event.target.checked })} />
              <span>{play.random}</span>
            </label>
            <span className="play-inline">
              <NumberField value={form.randomWithin} valid={whole(form.randomWithin, 1) !== null} disabled={!form.random} onChange={(randomWithin) => set({ randomWithin })} unit={play.randomWithin} testId="match-random-within" />
              <NumberField value={form.randomDrop} valid={decimal(form.randomDrop, 0, 100) !== null} disabled={!form.random} onChange={(randomDrop) => set({ randomDrop })} unit={play.randomDrop} testId="match-random-drop" />
              <NumberField value={form.randomShare} valid={decimal(form.randomShare, 0, 100) !== null} disabled={!form.random} onChange={(randomShare) => set({ randomShare })} unit={play.randomShare} testId="match-random-share" />
            </span>
          </div>
          <label className="settings-check">
            <input type="checkbox" checked={form.autoSave} data-testid="match-autosave" onChange={(event) => set({ autoSave: event.target.checked })} />
            <span>{play.autoSave}</span>
          </label>
        </div>
        <footer className="dialog-footer settings-footer">
          {error ? (
            <p className="play-error" role="alert" data-testid="match-error" title={error}>
              <strong>{labels.failed}</strong> {error}
            </p>
          ) : null}
          <button type="button" className="secondary-button" onClick={onClose}>
            {labels.cancel}
          </button>
          <button type="submit" className="primary-button" data-testid="match-begin" disabled={!ready}>
            {labels.begin}
          </button>
        </footer>
      </form>
    </PlayWindow>
  );
}
