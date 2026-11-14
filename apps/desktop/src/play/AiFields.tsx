import type { ReactNode } from "react";
import type { PlayLabels } from "./labels";
import { type AiForm, decimal, GTP_MODES, type GtpMode, gtpTime, whole } from "./setup";

export interface EngineChoice {
  value: string;
  label: string;
  gtp: boolean;
}

interface AiFieldsProps {
  form: AiForm;
  onChange(form: AiForm): void;
  choices: readonly EngineChoice[];
  labels: PlayLabels;
  /** The engine picker's own label: 对手 in 人机对局, 引擎 in 引擎对局. */
  engineLabel: string;
  /** Resignation and random opening, which 修改AI用时 leaves out. */
  more?: boolean;
  /** 引擎对局 sets the random opening for the whole match instead. */
  random?: boolean;
  /** Extra KataGo-only fields, e.g. 引擎对局's WRN and PDA. */
  extra?: ReactNode;
  testId: string;
}

/** A number field that marks itself invalid; the dialog decides whether it may start. */
export function NumberField({ value, valid, onChange, testId, disabled, unit, wide }: { value: string; valid: boolean; onChange(value: string): void; testId: string; disabled?: boolean; unit?: string; wide?: boolean }) {
  return (
    <span className="play-number">
      <input
        className={wide ? "text-input wide" : "text-input"}
        inputMode="decimal"
        value={value}
        disabled={disabled}
        aria-invalid={!disabled && !valid}
        data-testid={testId}
        onChange={(event) => onChange(event.target.value)}
      />
      {unit ? <span className="play-unit">{unit}</span> : null}
    </span>
  );
}

/** The AI's side of the dialog: KataGo's per-move limits, or a GTP engine's time control. */
export function AiFields({ form, onChange, choices, labels, engineLabel, more, random = true, extra, testId }: AiFieldsProps) {
  const set = (patch: Partial<AiForm>) => onChange({ ...form, ...patch });
  const setGtp = (patch: Partial<AiForm["gtp"]>) => onChange({ ...form, gtp: { ...form.gtp, ...patch } });
  const gtp = choices.find((choice) => choice.value === form.engine)?.gtp ?? false;
  const g = form.gtp;
  return (
    <div className="play-fields" data-testid={testId}>
      <label className="play-row">
        <span className="play-label">{engineLabel}</span>
        <select className="select" value={form.engine} data-testid={`${testId}-engine`} onChange={(event) => set({ engine: event.target.value })}>
          {choices.map((choice) => (
            <option key={choice.value} value={choice.value}>
              {choice.label}
            </option>
          ))}
        </select>
      </label>
      {gtp ? (
        <>
          <label className="play-row">
            <span className="play-label">{labels.gtpTime}</span>
            <select className="select" value={g.mode} data-testid={`${testId}-mode`} onChange={(event) => setGtp({ mode: event.target.value as GtpMode })}>
              {GTP_MODES.map((mode) => (
                <option key={mode} value={mode}>
                  {labels.modes[mode]}
                </option>
              ))}
            </select>
          </label>
          {g.mode === "PerMove" ? (
            <div className="play-row">
              <span className="play-label">{labels.modes.PerMove}</span>
              <NumberField value={g.perMove} valid={whole(g.perMove, 1) !== null} onChange={(perMove) => setGtp({ perMove })} unit={labels.seconds} testId={`${testId}-per-move`} />
            </div>
          ) : null}
          {g.mode === "Byoyomi" || g.mode === "Fischer" || g.mode === "Absolute" ? (
            <div className="play-row">
              <span className="play-label">{labels.mainTime}</span>
              <span className="play-inline">
                <NumberField value={g.main} valid={decimal(g.main, 0) !== null} onChange={(main) => setGtp({ main })} unit={labels.minutes} testId={`${testId}-main`} />
                {g.mode === "Byoyomi" ? (
                  <>
                    <NumberField value={g.period} valid={whole(g.period) !== null} onChange={(period) => setGtp({ period })} unit={labels.seconds} testId={`${testId}-period`} />
                    <NumberField value={g.stones} valid={whole(g.stones, 1) !== null} onChange={(stones) => setGtp({ stones })} unit={labels.stones} testId={`${testId}-stones`} />
                  </>
                ) : null}
                {g.mode === "Fischer" ? <NumberField value={g.increment} valid={whole(g.increment) !== null} onChange={(increment) => setGtp({ increment })} unit={labels.increment} testId={`${testId}-increment`} /> : null}
              </span>
            </div>
          ) : null}
          {g.mode === "Custom" ? (
            <div className="play-row play-column">
              <textarea className="text-input play-commands" rows={3} value={g.custom} aria-invalid={gtpTime(g) === null} data-testid={`${testId}-custom`} onChange={(event) => setGtp({ custom: event.target.value })} />
              <span className="settings-hint">{labels.customHint}</span>
            </div>
          ) : null}
        </>
      ) : (
        <>
          <div className="play-row">
            <span className="play-label">{labels.aiTime}</span>
            <span className="play-inline">
              <span className="play-pair">
                <label className="settings-check">
                  <input type="checkbox" checked={form.useSeconds} data-testid={`${testId}-use-seconds`} onChange={(event) => set({ useSeconds: event.target.checked })} />
                  <span>{labels.perMoveSeconds}</span>
                </label>
                <NumberField value={form.seconds} valid={decimal(form.seconds, 0.01) !== null} disabled={!form.useSeconds} onChange={(seconds) => set({ seconds, useSeconds: true })} testId={`${testId}-seconds`} />
              </span>
              <span className="play-pair">
                <label className="settings-check">
                  <input type="checkbox" checked={form.useVisits} data-testid={`${testId}-use-visits`} onChange={(event) => set({ useVisits: event.target.checked })} />
                  <span>{labels.perMoveVisits}</span>
                </label>
                <NumberField value={form.visits} valid={whole(form.visits, 1) !== null} disabled={!form.useVisits} onChange={(visits) => set({ visits, useVisits: true })} wide testId={`${testId}-visits`} />
              </span>
            </span>
          </div>
          <label className="settings-check play-indent">
            <input type="checkbox" checked={form.pure} data-testid={`${testId}-pure`} onChange={(event) => set({ pure: event.target.checked })} />
            <span>{labels.pureNetwork}</span>
          </label>
          {!form.pure && !form.useSeconds && !form.useVisits ? <p className="settings-hint play-invalid">{labels.needLimit}</p> : null}
          {extra}
          {more ? (
            <>
              <div className="play-row">
                <label className="settings-check play-label">
                  <input type="checkbox" checked={form.resign} data-testid={`${testId}-resign`} onChange={(event) => set({ resign: event.target.checked })} />
                  <span>{labels.resign}</span>
                </label>
                <span className="play-inline">
                  <NumberField value={form.resignAfter} valid={whole(form.resignAfter) !== null} disabled={!form.resign} onChange={(resignAfter) => set({ resignAfter })} unit={labels.resignAfter} testId={`${testId}-resign-after`} />
                  <NumberField value={form.resignCount} valid={whole(form.resignCount, 1) !== null} disabled={!form.resign} onChange={(resignCount) => set({ resignCount })} unit={labels.resignCount} testId={`${testId}-resign-count`} />
                  <NumberField value={form.resignBelow} valid={decimal(form.resignBelow, 0, 100) !== null} disabled={!form.resign} onChange={(resignBelow) => set({ resignBelow })} unit={labels.resignBelow} testId={`${testId}-resign-below`} />
                </span>
              </div>
              {random ? (
              <div className="play-row">
                <label className="settings-check play-label">
                  <input type="checkbox" checked={form.random} data-testid={`${testId}-random`} onChange={(event) => set({ random: event.target.checked })} />
                  <span>{labels.random}</span>
                </label>
                <span className="play-inline">
                  <NumberField value={form.randomWithin} valid={whole(form.randomWithin, 1) !== null} disabled={!form.random} onChange={(randomWithin) => set({ randomWithin })} unit={labels.randomWithin} testId={`${testId}-random-within`} />
                  <NumberField value={form.randomDrop} valid={decimal(form.randomDrop, 0, 100) !== null} disabled={!form.random} onChange={(randomDrop) => set({ randomDrop })} unit={labels.randomDrop} testId={`${testId}-random-drop`} />
                  <NumberField value={form.randomShare} valid={decimal(form.randomShare, 0, 100) !== null} disabled={!form.random} onChange={(randomShare) => set({ randomShare })} unit={labels.randomShare} testId={`${testId}-random-share`} />
                </span>
              </div>
              ) : null}
            </>
          ) : null}
        </>
      )}
    </div>
  );
}

