import { type ReactNode, useEffect, useState } from "react";
import type { Confidence, SideReport, StrengthEstimateView, WholeGameProgress } from "@/api/backend";
import { type BandLabels, bandText, percent } from "./strength";
import type { PhaseLabels } from "./phases";
import { StrengthPhases } from "./StrengthPhases";

export interface StrengthLabels {
  title: string;
  subtitle: string;
  close: string;
  loading: string;
  moves: string;
  sigma: string;
  sigmaHint: string;
  band: string;
  confidence: string;
  confidenceLevels: Record<Confidence, string>;
  match: string;
  firstChoice: string;
  top3: string;
  goodMoves: string;
  avgScoreLoss: string;
  avgWinrateLoss: string;
  mistakeRate: string;
  points: string;
  tiers: string;
  assessment: string;
  phases: string;
  phaseNames: Record<"opening" | "middlegame" | "endgame", string>;
  phaseRange: string;
  unplayed: string;
  avgLossShort: string;
  phase: PhaseLabels;
  help: {
    rank: string;
    band: string;
    confidence: string;
    match: string;
    firstChoice: string;
    top3: string;
    goodMoves: string;
    avgScoreLoss: string;
    avgWinrateLoss: string;
    mistakeRate: string;
  };
  emptyTitle: string;
  emptyBody: string;
  shortTitle: string;
  shortBody: string;
  fewTitle: string;
  fewBody: string;
  analyze: string;
  running: string;
  partial: string;
  note: string;
  bands: BandLabels;
}

export interface PlayerName {
  name: string;
  rank: string | null;
}

export interface StrengthDialogProps {
  view: StrengthEstimateView | null;
  progress: WholeGameProgress | null;
  black: PlayerName;
  white: PlayerName;
  /** Rust formats the rank in both languages; the UI picks one. */
  chinese: boolean;
  labels: StrengthLabels;
  /** Display names of the six move tiers, best first. */
  tierNames: readonly string[];
  /** Phase boundaries the user dragged to, or null for the defaults. */
  bounds: readonly [number, number] | null;
  onBounds(bounds: [number, number] | null): void;
  onAnalyze(): void;
  onClose(): void;
}

const TIER_CLASSES = ["best", "good", "normal", "inaccuracy", "mistake", "blunder"] as const;

/** LizzieYzy Next's strength window: both sides' estimated level, and by phase on its own page. */
export function StrengthDialog(props: StrengthDialogProps) {
  const { view, progress, black, white, chinese, labels, tierNames, bounds, onBounds, onAnalyze, onClose } = props;
  const [tab, setTab] = useState<"assessment" | "phases">("assessment");
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const running = progress?.status.type === "Running" || progress?.status.type === "Paused";
  const runningText = progress
    ? labels.running.replace("{done}", String(progress.completed)).replace("{total}", String(progress.total))
    : "";
  const analyzeAction = running ? (
    <p className="strength-running" data-testid="strength-running">
      <span className="spinner small" aria-hidden="true" />
      {runningText}
    </p>
  ) : (
    <button type="button" className="primary-button" data-testid="strength-analyze" onClick={onAnalyze}>
      {labels.analyze}
    </button>
  );

  let body;
  if (!view) {
    body = (
      <div className="fox-state" data-testid="strength-loading">
        <span className="spinner" aria-hidden="true" />
        <p>{labels.loading}</p>
      </div>
    );
  } else if (view.total_moves === 0 || (!view.report.has_enough_data && view.analyzed_moves >= view.total_moves)) {
    body = <State testId="strength-short" title={labels.shortTitle} body={labels.shortBody} />;
  } else if (view.analyzed_moves === 0) {
    body = <State testId="strength-empty" title={labels.emptyTitle} body={labels.emptyBody} action={analyzeAction} />;
  } else if (!view.report.has_enough_data) {
    body = <State testId="strength-few" title={labels.fewTitle} body={labels.fewBody} action={analyzeAction} />;
  } else {
    const partial = view.analyzed_moves < view.total_moves;
    body = (
      <div className="strength-report" data-testid="strength-report">
        {partial ? (
          <div className="strength-coverage" data-testid="strength-partial">
            <p>
              {labels.partial.replace("{done}", String(view.analyzed_moves)).replace("{total}", String(view.total_moves))}
            </p>
            {analyzeAction}
          </div>
        ) : null}
        <div className="tabs strength-tabs" role="tablist">
          {(["assessment", "phases"] as const).map((key) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={tab === key}
              data-testid={`strength-tab-${key}`}
              onClick={() => setTab(key)}
            >
              {labels[key]}
            </button>
          ))}
        </div>
        {tab === "assessment" ? (
          <>
            <div className="strength-sides">
              <SideCard color="black" player={black} side={view.report.black} chinese={chinese} labels={labels} tierNames={tierNames} />
              <SideCard color="white" player={white} side={view.report.white} chinese={chinese} labels={labels} tierNames={tierNames} />
            </div>
            <p className="strength-note">{labels.note}</p>
          </>
        ) : (
          <StrengthPhases
            report={view.phases}
            requested={bounds}
            lastMove={view.total_moves}
            samples={[...view.report.black.samples, ...view.report.white.samples]}
            names={{ black: black.name, white: white.name }}
            chinese={chinese}
            labels={labels}
            onBounds={onBounds}
          />
        )}
      </div>
    );
  }

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog strength-dialog" role="dialog" aria-modal="true" aria-labelledby="strength-title" data-testid="strength-dialog">
        <header className="dialog-header">
          <div>
            <h2 id="strength-title">{labels.title}</h2>
            <p className="strength-subtitle">{labels.subtitle}</p>
          </div>
          <button type="button" className="dialog-close" aria-label={labels.close} data-testid="strength-close" onClick={onClose}>
            ×
          </button>
        </header>
        <div className="strength-body" aria-live="polite">
          {body}
        </div>
      </section>
    </div>
  );
}

function State({ testId, title, body, action }: { testId: string; title: string; body: string; action?: ReactNode }) {
  return (
    <div className="fox-state" data-testid={testId}>
      <h3 className="strength-state-title">{title}</h3>
      <p>{body}</p>
      {action}
    </div>
  );
}

interface SideCardProps {
  color: "black" | "white";
  player: PlayerName;
  side: SideReport;
  chinese: boolean;
  labels: StrengthLabels;
  tierNames: readonly string[];
}

function SideCard({ color, player, side, chinese, labels, tierNames }: SideCardProps) {
  const rank = chinese ? side.rank_label_zh : side.rank_label_en;
  const band = bandText(side.strength_band, labels.bands);
  const level = side.confidence === "High" ? 3 : side.confidence === "Medium" ? 2 : 1;
  const total = side.tier_counts.reduce((sum, count) => sum + count, 0);
  const scoreLoss = side.average_score_loss;

  return (
    <article className="strength-card" data-testid={`strength-${color}`}>
      <header className="strength-card-header">
        <span className={`stone-icon ${color}`} aria-hidden="true" />
        <span className="strength-player">
          {player.name}
          {player.rank ? <span className="player-rank">{player.rank}</span> : null}
        </span>
        <span className="strength-moves">{labels.moves.replace("{n}", String(side.sample_count))}</span>
      </header>

      <div className="strength-rank" title={labels.help.rank}>
        <strong data-testid={`strength-${color}-rank`}>{side.rank_value === null ? "—" : rank}</strong>
        {side.prediction_sigma !== null && side.rank_value !== null ? (
          <span className="strength-sigma" title={labels.sigmaHint.replace("{n}", side.prediction_sigma.toFixed(1))}>
            {labels.sigma.replace("{n}", side.prediction_sigma.toFixed(1))}
          </span>
        ) : null}
      </div>
      <p className="strength-meta">
        {band ? (
          <span title={labels.help.band}>
            {labels.band} <b>{band}</b>
          </span>
        ) : null}
        <span className="strength-confidence" title={labels.help.confidence} data-level={level}>
          {labels.confidence}
          <span className="confidence-bars" aria-hidden="true">
            <i />
            <i />
            <i />
          </span>
          <b>{labels.confidenceLevels[side.confidence]}</b>
        </span>
      </p>

      <dl className="strength-key">
        <Stat label={labels.match} value={percent(side.match_rate)} help={labels.help.match} accent testId={`strength-${color}-match`} />
        <Stat label={labels.firstChoice} value={percent(side.first_choice_rate)} help={labels.help.firstChoice} />
        <Stat label={labels.top3} value={percent(side.top3_rate)} help={labels.help.top3} />
      </dl>
      <dl className="strength-more">
        <Row
          label={labels.avgScoreLoss}
          value={scoreLoss === null ? "—" : labels.points.replace("{n}", scoreLoss.toFixed(2))}
          help={labels.help.avgScoreLoss}
        />
        <Row label={labels.avgWinrateLoss} value={`${side.average_winrate_loss.toFixed(1)}%`} help={labels.help.avgWinrateLoss} />
        <Row label={labels.goodMoves} value={percent(side.good_move_rate)} help={labels.help.goodMoves} />
        <Row label={labels.mistakeRate} value={percent(side.mistake_rate)} help={labels.help.mistakeRate} />
      </dl>

      <div className="strength-tiers">
        <span className="strength-section-label">{labels.tiers}</span>
        <div className="tier-bar" aria-hidden="true">
          {side.tier_counts.map((count, index) =>
            count > 0 ? (
              <span key={TIER_CLASSES[index]} className={`tier ${TIER_CLASSES[index]}`} style={{ flexGrow: total > 0 ? count : 0 }} />
            ) : null,
          )}
        </div>
        <ul className="tier-legend">
          {side.tier_counts.map((count, index) => (
            <li key={TIER_CLASSES[index]} data-testid={`strength-${color}-tier`}>
              <span className={`tier-dot ${TIER_CLASSES[index]}`} aria-hidden="true" />
              {tierNames[index]}
              <b>{count}</b>
            </li>
          ))}
        </ul>
      </div>
    </article>
  );
}

function Stat({ label, value, help, accent, testId }: { label: string; value: string; help: string; accent?: boolean; testId?: string }) {
  return (
    <div className={accent ? "strength-stat accent" : "strength-stat"} title={help}>
      <dd data-testid={testId}>{value}</dd>
      <dt>{label}</dt>
    </div>
  );
}

function Row({ label, value, help }: { label: string; value: string; help: string }) {
  return (
    <div className="strength-row" title={help}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
