import { type FormEvent, useState } from "react";
import type { MatchSide, MatchView } from "../api/match-game";
import { reasonOf } from "../everyday/useEveryday";
import { NumberField } from "./AiFields";
import type { MatchLabels } from "./labels";
import { PlayWindow } from "./PlayDialog";
import { eloText } from "./shared";
import { decimal, whole } from "./setup";

export interface MatchControls {
  pause(): void;
  resume(): void;
  stop(): void;
  setGames(): void;
  playNow(): void;
  resign(color: "Black" | "White"): void;
  setParams(side: MatchSide, wrn: number, pda: number): void;
  close(): void;
}

const baseName = (path: string) => path.split(/[\\/]/).at(-1) ?? path;



interface MatchPanelProps {
  view: MatchView;
  names: readonly [string, string];
  /** Which sides are KataGo, whose WRN and PDA can change mid-match. */
  katago: readonly [boolean, boolean];
  labels: MatchLabels;
  controls: MatchControls;
}

/** The score window: totals by colour, unscored games, Elo, per opening, and the match controls. */
export function MatchPanel({ view, names, katago, labels, controls }: MatchPanelProps) {
  const [intervene, setIntervene] = useState(false);
  const [params, setParams] = useState<[{ wrn: string; pda: string }, { wrn: string; pda: string }]>([
    { wrn: "0", pda: "0" },
    { wrn: "0", pda: "0" },
  ]);
  const phase = view.phase ?? "Running";
  const active = phase === "Running" || phase === "Paused";
  const unscored = Object.entries(view.unscored ?? {}).filter(([, count]) => (count ?? 0) > 0);
  const unscoredTotal = unscored.reduce((sum, [, count]) => sum + (count ?? 0), 0);
  const row = (index: 0 | 1) => {
    const wins = (index === 0 ? view.a : view.b) ?? {};
    return (
      <tr key={index} data-testid={`match-row-${index + 1}`}>
        <th scope="row" title={names[index]}>
          <span className="match-name">{names[index]}</span>
        </th>
        <td className="match-total">{wins.total ?? 0}</td>
        <td>{wins.black ?? 0}</td>
        <td>{wins.white ?? 0}</td>
      </tr>
    );
  };
  const apply = (index: 0 | 1) => (event: FormEvent) => {
    event.preventDefault();
    const wrn = decimal(params[index].wrn, 0, 5);
    const pda = decimal(params[index].pda, -3, 3);
    if (wrn !== null && pda !== null) controls.setParams(index === 0 ? "A" : "B", wrn, pda);
  };
  return (
    <aside className="match-panel" role="dialog" aria-labelledby="match-panel-title" data-testid="match-panel" data-phase={phase}>
      <header className="match-panel-header">
        <h2 id="match-panel-title">{labels.panelTitle}</h2>
        <span className="match-phase" data-phase={phase}>
          {labels.phases[phase]}
        </span>
        <button type="button" className="dialog-close" aria-label={labels.close} onClick={controls.close}>
          ×
        </button>
      </header>
      <p className="match-progress" data-testid="match-progress">
        {labels.progress.replace("{current}", String(Math.max(view.current_game ?? 0, view.completed ?? 0))).replace("{games}", String(view.games ?? 0))}
        {view.current_opening ? <span title={view.current_opening}> · {baseName(view.current_opening)}</span> : null}
      </p>
      <table className="match-table">
        <thead>
          <tr>
            <th />
            <th>{labels.wins}</th>
            <th>{labels.asBlack}</th>
            <th>{labels.asWhite}</th>
          </tr>
        </thead>
        <tbody>
          {row(0)}
          {row(1)}
        </tbody>
      </table>
      <dl className="match-facts">
        <dt>{labels.draws}</dt>
        <dd>{view.draws ?? 0}</dd>
        <dt>{labels.unscored}</dt>
        <dd title={unscored.map(([reason, count]) => `${reason} ×${count}`).join("\n")}>
          {unscoredTotal}
          {unscored.length > 0 ? <small> ({unscored.map(([reason, count]) => `${reason} ×${count}`).join("，")})</small> : null}
        </dd>
        <dt>{labels.elo}</dt>
        <dd data-testid="match-elo">{eloText(view, labels)}</dd>
      </dl>
      {(view.openings ?? []).length > 0 ? (
        <table className="match-table match-openings">
          <thead>
            <tr>
              <th>{labels.opening}</th>
              <th>1</th>
              <th>2</th>
              <th>{labels.draws}</th>
            </tr>
          </thead>
          <tbody>
            {(view.openings ?? []).map((standing) => (
              <tr key={standing.opening}>
                <th scope="row" title={standing.opening}>
                  <span className="match-name">{baseName(standing.opening ?? "")}</span>
                </th>
                <td>{standing.a?.total ?? 0}</td>
                <td>{standing.b?.total ?? 0}</td>
                <td>{standing.draws ?? 0}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
      {view.error ? (
        <p className="play-error" data-testid="match-error-line">
          {reasonOf(view.error)}
        </p>
      ) : null}
      {view.save_directory ? (
        <p className="settings-hint match-saved" title={view.save_directory}>
          {labels.savedIn.replace("{path}", view.save_directory)}
        </p>
      ) : null}
      {active && intervene ? (
        <div className="match-intervene" data-testid="match-intervene">
          <div className="play-inline">
            <button type="button" className="secondary-button" data-testid="match-play-now" onClick={controls.playNow}>
              {labels.playNow}
            </button>
            <button type="button" className="secondary-button" onClick={() => controls.resign("Black")}>
              {labels.resignBlack}
            </button>
            <button type="button" className="secondary-button" onClick={() => controls.resign("White")}>
              {labels.resignWhite}
            </button>
          </div>
          {([0, 1] as const)
            .filter((index) => katago[index])
            .map((index) => (
              <form key={index} className="play-inline" onSubmit={apply(index)}>
                <span className="play-label">{labels.side.replace("{n}", String(index + 1))}</span>
                <span className="play-unit">WRN</span>
                <NumberField value={params[index].wrn} valid={decimal(params[index].wrn, 0, 5) !== null} onChange={(wrn) => setParams((current) => current.map((p, at) => (at === index ? { ...p, wrn } : p)) as typeof current)} testId={`match-wrn-${index + 1}`} />
                <span className="play-unit">PDA</span>
                <NumberField value={params[index].pda} valid={decimal(params[index].pda, -3, 3) !== null} onChange={(pda) => setParams((current) => current.map((p, at) => (at === index ? { ...p, pda } : p)) as typeof current)} testId={`match-pda-${index + 1}`} />
                <button type="submit" className="secondary-button">
                  {labels.apply}
                </button>
              </form>
            ))}
        </div>
      ) : null}
      {active ? (
        <footer className="match-controls">
          {phase === "Paused" ? (
            <button type="button" className="secondary-button" data-testid="match-resume" onClick={controls.resume}>
              {labels.resume}
            </button>
          ) : (
            <button type="button" className="secondary-button" data-testid="match-pause" onClick={controls.pause}>
              {labels.pause}
            </button>
          )}
          <button type="button" className="secondary-button" onClick={controls.setGames}>
            {labels.games}
          </button>
          <button type="button" className="secondary-button" aria-pressed={intervene} data-testid="match-intervene-toggle" onClick={() => setIntervene((open) => !open)}>
            {labels.interveneTitle}
          </button>
          <button type="button" className="danger-link" data-testid="match-stop" onClick={controls.stop}>
            {labels.stop}
          </button>
        </footer>
      ) : null}
    </aside>
  );
}

/** 棋局 → 修改多盘引擎对局数. */
export function GamesDialog({ current, done, labels, onApply, onClose }: { current: number; done: number; labels: MatchLabels; onApply(games: number): void; onClose(): void }) {
  const [games, setGames] = useState(String(current));
  const value = whole(games, 1, 10_000);
  return (
    <PlayWindow title={labels.gamesTitle} testId="match-games-dialog" onClose={onClose} close={labels.close}>
      <form
        className="play-form"
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          if (value !== null) onApply(value);
        }}
      >
        <div className="play-row">
          <span className="play-label">{labels.games}</span>
          <NumberField value={games} valid={value !== null} onChange={setGames} testId="match-games-input" />
          <span className="settings-hint">{labels.progress.replace("{current}", String(done)).replace("{games}", String(current))}</span>
        </div>
        <footer className="dialog-footer settings-footer">
          <button type="button" className="secondary-button" onClick={onClose}>
            {labels.cancel}
          </button>
          <button type="submit" className="primary-button" disabled={value === null}>
            {labels.apply}
          </button>
        </footer>
      </form>
    </PlayWindow>
  );
}
