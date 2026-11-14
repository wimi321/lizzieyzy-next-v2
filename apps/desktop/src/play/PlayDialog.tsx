import { type FormEvent, type ReactNode, useState } from "react";
import { reasonOf } from "../everyday/useEveryday";
import type { AiPlayer, GameSetup, GameView } from "../api/game";
import type { GtpEngineEntry } from "../api/gtp";
import { RULE_PRESETS } from "../edit/info";
import { AiFields, NumberField } from "./AiFields";
import { aiValid, engineChoices, useEscape } from "./shared";
import type { PlayLabels, RuleLabels } from "./labels";
import { aiForm, aiPlayer, decimal, gameForm, type GameForm, gameSetup, resultText, whole } from "./setup";

/** 对弈模式: the dialog's display choice, kept for this session. */
export interface FocusOptions {
  focus: boolean;
  showBlack: boolean;
  showWhite: boolean;
}


/** The dialog chrome every play window shares. */
export function PlayWindow({ title, testId, className, onClose, close, children }: { title: string; testId: string; className?: string; onClose(): void; close: string; children: ReactNode }) {
  useEscape(onClose);
  return (
    <div className="dialog-backdrop" role="presentation">
      <section className={`dialog play-dialog ${className ?? ""}`} role="dialog" aria-modal="true" aria-labelledby={`${testId}-title`} data-testid={testId}>
        <header className="dialog-header">
          <h2 id={`${testId}-title`}>{title}</h2>
          <button type="button" className="dialog-close" aria-label={close} onClick={onClose}>
            ×
          </button>
        </header>
        {children}
      </section>
    </div>
  );
}

export function Segmented<T extends string | number>({ value, options, onChange, label, testId }: { value: T; options: readonly { value: T; label: string; disabled?: boolean }[]; onChange(value: T): void; label: string; testId: string }) {
  return (
    <div className="segmented small" role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <button key={option.value} type="button" role="radio" aria-checked={value === option.value} disabled={option.disabled} data-testid={`${testId}-${option.value}`} onClick={() => onChange(option.value)}>
          <span>{option.label}</span>
        </button>
      ))}
    </div>
  );
}


interface PlayDialogProps {
  saved: GameSetup | null;
  gtpEngines: readonly GtpEngineEntry[];
  /** There is a game on the board to continue. */
  canContinue: boolean;
  focus: FocusOptions;
  labels: PlayLabels;
  rules: RuleLabels;
  /** Resolves with the reason the game could not start, or null once it has. */
  onStart(setup: GameSetup, focus: FocusOptions): Promise<string | null>;
  onClose(): void;
}

/** 棋局 → 新对局 → 人机对局: Java's genmove and analysis-mode dialogs in one. */
export function PlayDialog({ saved, gtpEngines, canContinue, focus: savedFocus, labels, rules, onStart, onClose }: PlayDialogProps) {
  const initial = gameForm(saved);
  const [game, setGame] = useState<GameForm>({ ...initial.game, continueHere: initial.game.continueHere && canContinue });
  const [ai, setAi] = useState(initial.ai);
  const [focus, setFocus] = useState(savedFocus);
  const [more, setMore] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<GameForm>) => setGame((current) => ({ ...current, ...patch }));
  const choices = engineChoices(labels.katago, gtpEngines);
  const gtp = choices.find((choice) => choice.value === ai.engine)?.gtp ?? false;
  const setup = gameSetup({ ...game, free: game.free && gtp }, ai, saved);
  const ready = setup !== null && aiValid(ai, choices) && !busy;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!setup || busy) return;
    setBusy(true);
    setError(null);
    void onStart(setup, focus).then((failure) => {
      setBusy(false);
      if (failure) setError(failure);
    });
  };

  const handicaps = [0, 2, 3, 4, 5, 6, 7, 8, 9];
  return (
    <PlayWindow title={labels.title} testId="play-dialog" onClose={onClose} close={labels.close}>
      <form className="play-form" noValidate onSubmit={submit}>
        <div className="play-panel">
          <div className="play-row">
            <span className="play-label">{labels.youPlay}</span>
            <Segmented value={game.human} label={labels.youPlay} testId="play-human" onChange={(human) => set({ human })} options={[{ value: "Black", label: labels.black }, { value: "White", label: labels.white }]} />
          </div>
          <div className="play-row">
            <span className="play-label">{labels.startFrom}</span>
            <Segmented
              value={game.continueHere ? "here" : "new"}
              label={labels.startFrom}
              testId="play-start"
              onChange={(value) => set({ continueHere: value === "here" })}
              options={[
                { value: "new", label: labels.newBoard },
                { value: "here", label: labels.currentPosition, disabled: !canContinue },
              ]}
            />
          </div>
          {game.continueHere ? null : (
            <>
              <div className="play-row">
                <span className="play-label">{labels.size}</span>
                <Segmented value={game.size} label={labels.size} testId="play-size" onChange={(size) => set({ size })} options={[19, 13, 9].map((size) => ({ value: size, label: `${size}×${size}` }))} />
              </div>
              <div className="play-row">
                <span className="play-label">{labels.handicap}</span>
                <span className="play-inline">
                  <select className="select" value={game.handicap} data-testid="play-handicap" onChange={(event) => set({ handicap: Number(event.target.value) })}>
                    {handicaps.map((count) => (
                      <option key={count} value={count}>
                        {count === 0 ? labels.noHandicap : count}
                      </option>
                    ))}
                  </select>
                  <label className="settings-check" title={labels.freeHint}>
                    <input type="checkbox" checked={game.free && gtp} disabled={!gtp || game.handicap < 2} data-testid="play-free" onChange={(event) => set({ free: event.target.checked })} />
                    <span>{labels.freeHandicap}</span>
                  </label>
                </span>
              </div>
              <div className="play-row">
                <span className="play-label">{labels.komi}</span>
                <span className="play-inline">
                  <NumberField value={game.komi} valid={decimal(game.komi, -1000) !== null} onChange={(komi) => set({ komi })} testId="play-komi" />
                  <span className="play-label play-gap">{labels.rules}</span>
                  <select className="select" value={game.rules} data-testid="play-rules" onChange={(event) => set({ rules: event.target.value })}>
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
              <input type="checkbox" checked={game.clock} data-testid="play-clock" onChange={(event) => set({ clock: event.target.checked })} />
              <span>{labels.myClock}</span>
            </label>
            <span className="play-inline">
              <NumberField value={game.clockMain} valid={decimal(game.clockMain, 0) !== null} disabled={!game.clock} onChange={(clockMain) => set({ clockMain })} unit={labels.minutes} testId="play-clock-main" />
              <span className="play-unit">{labels.byoyomi}</span>
              <NumberField value={game.clockSeconds} valid={whole(game.clockSeconds) !== null} disabled={!game.clock} onChange={(clockSeconds) => set({ clockSeconds })} unit={labels.seconds} testId="play-clock-seconds" />
              <span className="play-unit">×</span>
              <NumberField value={game.clockPeriods} valid={whole(game.clockPeriods) !== null} disabled={!game.clock} onChange={(clockPeriods) => set({ clockPeriods })} unit={labels.periods} testId="play-clock-periods" />
            </span>
          </div>
        </div>

        <div className="play-panel">
          <AiFields form={ai} onChange={setAi} choices={choices} labels={labels} engineLabel={labels.opponent} more={more} testId="play-ai" />
        </div>

        <button type="button" className="play-more-toggle" aria-expanded={more} data-testid="play-more" onClick={() => setMore((open) => !open)}>
          {more ? "▾" : "▸"} {labels.more}
        </button>
        {more ? (
          <div className="play-panel play-checks">
            <label className="settings-check">
              <input type="checkbox" checked={game.ponder} data-testid="play-ponder" onChange={(event) => set({ ponder: event.target.checked })} />
              <span>{labels.ponder}</span>
            </label>
            <label className="settings-check">
              <input type="checkbox" checked={game.autoSave} data-testid="play-autosave" onChange={(event) => set({ autoSave: event.target.checked })} />
              <span>{labels.autoSave}</span>
            </label>
            <label className="settings-check">
              <input type="checkbox" checked={focus.focus} data-testid="play-focus" onChange={(event) => setFocus({ ...focus, focus: event.target.checked })} />
              <span>{labels.focus}</span>
            </label>
            <span className="play-inline play-indent">
              <label className="settings-check">
                <input type="checkbox" checked={focus.showBlack} disabled={!focus.focus} onChange={(event) => setFocus({ ...focus, showBlack: event.target.checked })} />
                <span>{labels.showBlack}</span>
              </label>
              <label className="settings-check">
                <input type="checkbox" checked={focus.showWhite} disabled={!focus.focus} onChange={(event) => setFocus({ ...focus, showWhite: event.target.checked })} />
                <span>{labels.showWhite}</span>
              </label>
            </span>
          </div>
        ) : null}

        <footer className="dialog-footer settings-footer">
          {error ? (
            <p className="play-error" role="alert" data-testid="play-error" title={error}>
              <strong>{labels.failed}</strong> {error}
            </p>
          ) : !ready && !busy ? (
            <p className="play-error">{labels.invalid}</p>
          ) : null}
          <button type="button" className="secondary-button" onClick={onClose}>
            {labels.cancel}
          </button>
          <button type="submit" className="primary-button" data-testid="play-begin" disabled={!ready}>
            {labels.begin}
          </button>
        </footer>
      </form>
    </PlayWindow>
  );
}

interface AiTimeDialogProps {
  game: GameView;
  gtpEngines: readonly GtpEngineEntry[];
  labels: PlayLabels;
  onApply(changes: readonly ["Black" | "White", AiPlayer][]): Promise<string | null>;
  onClose(): void;
}

/** 棋局 → 修改AI用时: each AI side's limits, from its next move on. */
export function AiTimeDialog({ game, gtpEngines, labels, onApply, onClose }: AiTimeDialogProps) {
  const sides = (["Black", "White"] as const).flatMap((color) => {
    const player = color === "Black" ? game.black : game.white;
    return player?.type === "Ai" ? [{ color, player: player.data }] : [];
  });
  const [forms, setForms] = useState(() => sides.map((side) => aiForm(side.player)));
  const [error, setError] = useState<string | null>(null);
  const choicesFor = (player: AiPlayer) => engineChoices(labels.katago, gtpEngines).filter((choice) => (player.type === "Gtp" ? choice.value === player.data.engine_id : !choice.gtp));
  const players = forms.map(aiPlayer);
  const ready = sides.length > 0 && players.every((player) => player !== null);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    void onApply(sides.map((side, index) => [side.color, players[index]!] as const)).then((failure) => (failure ? setError(failure) : onClose()));
  };
  return (
    <PlayWindow title={labels.aiTimeTitle} testId="ai-time-dialog" onClose={onClose} close={labels.close}>
      <form className="play-form" noValidate onSubmit={submit}>
        {sides.length === 0 ? <p className="settings-hint">{labels.errors.NotActive}</p> : null}
        {sides.map((side, index) => (
          <div className="play-panel" key={side.color}>
            <h3 className="dialog-subhead">{side.color === "Black" ? labels.black : labels.white}</h3>
            <AiFields
              form={forms[index]!}
              onChange={(form) => setForms((current) => current.map((old, at) => (at === index ? form : old)))}
              choices={choicesFor(side.player)}
              labels={labels}
              engineLabel={labels.opponent}
              testId={`ai-time-${side.color.toLowerCase()}`}
            />
          </div>
        ))}
        <footer className="dialog-footer settings-footer">
          {error ? <p className="play-error" role="alert">{error}</p> : null}
          <button type="button" className="secondary-button" onClick={onClose}>
            {labels.cancel}
          </button>
          <button type="submit" className="primary-button" data-testid="ai-time-apply" disabled={!ready}>
            {labels.apply}
          </button>
        </footer>
      </form>
    </PlayWindow>
  );
}

/** The game's end, in a corner card that leaves the board free to look at. */
export function GameNotice({ view, labels, onClose }: { view: GameView; labels: PlayLabels; onClose(): void }) {
  const result = resultText(view.result, labels);
  const reason = view.end_reason ? labels.reasons[view.end_reason] : null;
  const saveFailed = view.error?.type === "AutoSave" ? reasonOf(view.error) : null;
  return (
    <aside className="play-notice" role="status" data-testid="play-notice">
      <header>
        <strong>{labels.finished}</strong>
        <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
          ×
        </button>
      </header>
      <p className="play-result" data-testid="play-result">
        {result ?? reason}
      </p>
      {result && reason && !result.includes(reason) ? <p className="settings-hint">{reason}</p> : null}
      {saveFailed ? <p className="play-error">{saveFailed}</p> : null}
    </aside>
  );
}
