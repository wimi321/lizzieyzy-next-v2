import type { BoardView } from "@/api/backend";
import type { CoachBestMove, CoachDecision, CoachHint, CoachMoveRated, CoachView } from "../api/coach";
import type { GameView } from "../api/game";
import type { PlayLabels } from "../play/labels";
import { clockText, resultText } from "../play/setup";
import { changeFor, type CoachLabels, headline, moveText, percent, rankName, VERDICTS } from "./rank";

export interface CoachActions {
  hint(): void;
  closeHint(): void;
  takeBack(): void;
  pass(): void;
  resign(): void;
  end(): void;
  decide(decision: CoachDecision): void;
  reviewMove(node: number): void;
  review(): void;
  again(): void;
}

interface CoachPanelProps {
  view: CoachView;
  game: GameView | null;
  board: BoardView | null;
  ratings: readonly CoachMoveRated[];
  hint: CoachHint | null;
  best: CoachBestMove | null;
  labels: CoachLabels;
  play: PlayLabels;
  /** The board's word for a pass, for a suggested pass. */
  passLabel: string;
  actions: CoachActions;
}

/** The win chance after each rated move, from the human's side, with 恶手 in red. */
function Curve({ ratings, human }: { ratings: readonly CoachMoveRated[]; human: "Black" | "White" }) {
  const points = ratings
    .filter((rating) => rating.winrate_after !== undefined)
    .map((rating) => ({ value: human === "Black" ? rating.winrate_after! : 1 - rating.winrate_after!, blunder: rating.verdict === "Blunder" }));
  if (points.length === 0) return <div className="coach-curve empty" />;
  const width = 280;
  const height = 64;
  const x = (index: number) => (points.length === 1 ? width / 2 : (index / (points.length - 1)) * (width - 8) + 4);
  const y = (value: number) => height - 4 - value * (height - 8);
  return (
    <svg className="coach-curve" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" data-testid="coach-curve" aria-hidden="true">
      <line x1="0" x2={width} y1={y(0.5)} y2={y(0.5)} className="coach-curve-mid" />
      <polyline points={points.map((point, index) => `${x(index)},${y(point.value)}`).join(" ")} className="coach-curve-line" />
      {points.map((point, index) => (point.blunder ? <circle key={index} cx={x(index)} cy={y(point.value)} r="3.5" className="coach-curve-blunder" /> : null))}
    </svg>
  );
}

function RatingLine({ rating, labels, rank, head = true }: { rating: CoachMoveRated; labels: CoachLabels; rank: string; head?: boolean }) {
  const change = changeFor(rating);
  const share = percent(rating.human_share);
  return (
    <>
      {head ? (
        <div className="coach-rating-head">
          <span>{labels.moveNumber.replace("{n}", String(rating.move_number ?? 0))}</span>
          <span className="coach-verdict" data-verdict={rating.verdict}>
            {rating.verdict ? labels.verdicts[rating.verdict] : labels.rating}
          </span>
        </div>
      ) : null}
      <div className="coach-rating-facts">
        {change !== null ? <span>{labels.change.replace("{change}", `${change > 0 ? "+" : ""}${change}`)}</span> : null}
        {rating.points_lost && rating.points_lost >= 0.5 ? <span>{labels.lost.replace("{points}", rating.points_lost.toFixed(1))}</span> : null}
        {share !== null ? <span>{labels.share.replace("{share}", String(share)).replace("{rank}", rank)}</span> : null}
      </div>
    </>
  );
}

/** Java's coach panel, in the right column while the coach game is on and after it ends. */
export function CoachPanel({ view, game, board, ratings, hint, best, labels, play, passLabel, actions }: CoachPanelProps) {
  const setup = view.setup ?? {};
  const rank = rankName(setup.rank, labels);
  const human = view.human_color === "White" ? "White" : "Black";
  const summary = view.summary ?? null;
  const finished = summary !== null || view.active === false || game?.phase === "Finished";
  const humanTurn = game?.to_play === human && !game?.ai_thinking;
  const feedback = setup.mode === "LiveFeedback";
  const state = finished ? labels.over : view.awaiting_decision ? labels.deciding : game?.ai_thinking || !humanTurn ? labels.thinking : labels.yourTurn;
  const clock = clockText(human === "Black" ? game?.black_clock : game?.white_clock, play);
  const last = view.last_rating ?? ratings.at(-1) ?? null;
  const height = board?.height ?? 19;
  const counts = VERDICTS.map((verdict) => [verdict, ratings.filter((rating) => rating.verdict === verdict).length] as const).filter(([, count]) => count > 0);

  return (
    <section className="coach-panel" data-testid="coach-panel" data-mode={setup.mode}>
      <header className="coach-head">
        <div>
          <h2>{labels.panelTitle.replace("{rank}", rank).replace("{side}", human === "Black" ? labels.youBlack : labels.youWhite)}</h2>
          <p>{labels.playsLike.replace("{rank}", rank)}</p>
        </div>
        <span className="coach-state" data-testid="coach-state" data-state={finished ? "over" : view.awaiting_decision ? "deciding" : humanTurn ? "human" : "thinking"}>
          {state}
        </span>
      </header>

      <div className="coach-facts">
        <span>
          {labels.captures} ● {board?.captured_white ?? 0} ○ {board?.captured_black ?? 0}
        </span>
        {clock ? <span className="coach-clock">{clock}</span> : null}
        <span>{labels.komi.replace("{komi}", String(setup.handicap ? 0.5 : 7.5))}</span>
      </div>

      {summary ? (
        <div className="coach-summary" data-testid="coach-summary">
          <strong className="coach-headline">{headline(summary, view.human_color, labels, play.draw)}</strong>
          {summary.result ? <span className="coach-result">{resultText(summary.result, play)}</span> : summary.end_reason ? <span className="coach-result">{play.reasons[summary.end_reason]}</span> : null}
          <p className="coach-stats">
            {labels.stats
              .replace("{moves}", String(summary.total_moves ?? 0))
              .replace("{blunders}", String(summary.blunders ?? 0))
              .replace("{hints}", String(summary.hints ?? 0))
              .replace("{takeBacks}", String(summary.take_backs ?? 0))}
          </p>
          {(summary.worst_moves ?? []).length > 0 ? (
            <>
              <h3>{labels.worth}</h3>
              <ul className="coach-worst">
                {(summary.worst_moves ?? []).slice(0, 3).map((move) => (
                  <li key={move.node_id}>
                    <button type="button" data-testid="coach-worst" onClick={() => actions.reviewMove(move.node_id ?? 0)}>
                      <span>{labels.lastMove.replace("{n}", String(move.move_number ?? 0)).replace("{move}", move.verdict ? labels.verdicts[move.verdict] : "")}</span>
                      <span className="coach-loss">{labels.lost.replace("{points}", (move.points_lost ?? 0).toFixed(1))}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </>
          ) : null}
          <div className="coach-actions">
            <button type="button" className="primary-button" data-testid="coach-review" onClick={actions.review}>
              {labels.review}
            </button>
            <button type="button" className="secondary-button" data-testid="coach-again" onClick={actions.again}>
              {labels.again}
            </button>
          </div>
        </div>
      ) : (
        <>
          {view.awaiting_decision && last ? (
            <div className="coach-card coach-blunder" data-testid="coach-blunder" role="alert">
              <strong>{labels.blunderTitle.replace("{n}", String(last.move_number ?? 0)).replace("{verdict}", last.verdict ? labels.verdicts[last.verdict] : "")}</strong>
              <RatingLine rating={last} labels={labels} rank={rank} head={false} />
              {best ? (
                <p className="coach-best" data-testid="coach-best">
                  {labels.bestIs.replace("{move}", moveText(best.mv, height, passLabel))}
                  {percent(best.human_share) !== null ? ` · ${labels.share.replace("{share}", String(percent(best.human_share))).replace("{rank}", rank)}` : ""}
                </p>
              ) : null}
              <div className="coach-actions">
                <button type="button" className="primary-button" data-testid="coach-retry" onClick={() => actions.decide("Retry")}>
                  {labels.retry}
                </button>
                <button type="button" className="secondary-button" data-testid="coach-show-best" disabled={best !== null} onClick={() => actions.decide("ShowBest")}>
                  {labels.showBest}
                </button>
                <button type="button" className="secondary-button" data-testid="coach-continue" onClick={() => actions.decide("Continue")}>
                  {labels.carryOn}
                </button>
              </div>
            </div>
          ) : null}

          {hint ? (
            <div className="coach-card coach-hint" data-testid="coach-hint" data-step={hint.type}>
              <strong>
                {hint.type === "Reading"
                  ? labels.hintReading
                  : hint.type === "Region"
                    ? labels.hintRegion.replace("{area}", labels.areas[`${hint.data.row ?? 1}${hint.data.column ?? 1}` as keyof CoachLabels["areas"]] ?? labels.areas["11"])
                    : labels.hintExact.replace("{move}", moveText(hint.data.mv, height, passLabel))}
              </strong>
              {hint.type === "Exact" && percent(hint.data.human_share) !== null ? <span>{labels.share.replace("{share}", String(percent(hint.data.human_share))).replace("{rank}", rank)}</span> : null}
              <div className="coach-actions">
                {hint.type === "Region" ? (
                  <>
                    <button type="button" className="secondary-button" data-testid="coach-more-hint" onClick={actions.hint}>
                      {labels.moreHint}
                    </button>
                    <button type="button" className="secondary-button" onClick={actions.closeHint}>
                      {labels.myself}
                    </button>
                  </>
                ) : (
                  <button type="button" className="secondary-button" onClick={actions.closeHint}>
                    {labels.gotIt}
                  </button>
                )}
              </div>
            </div>
          ) : null}

          {feedback ? (
            <div className="coach-curve-box">
              <span className="coach-caption">{labels.curve.replace("{rank}", rank)}</span>
              <Curve ratings={ratings} human={human} />
            </div>
          ) : null}

          {!view.awaiting_decision && feedback && last ? (
            <div className="coach-card" data-testid="coach-last">
              <RatingLine rating={last} labels={labels} rank={rank} />
            </div>
          ) : null}

          {feedback && ratings.length > 0 ? (
            <div className="coach-history" data-testid="coach-history">
              <div className="coach-counts">
                {counts.map(([verdict, count]) => (
                  <span key={verdict} className="coach-verdict" data-verdict={verdict}>
                    {labels.verdicts[verdict]} {count}
                  </span>
                ))}
              </div>
              <h3>{labels.recent}</h3>
              <ol className="coach-recent">
                {ratings
                  .slice(-8)
                  .reverse()
                  .map((rating) => (
                    <li key={`${rating.node_id}-${rating.move_number}`}>
                      <span>{rating.move_number}</span>
                      <span className="coach-verdict" data-verdict={rating.verdict}>
                        {rating.verdict ? labels.verdicts[rating.verdict] : labels.rating}
                      </span>
                      <span className="coach-loss">{changeFor(rating) !== null ? `${changeFor(rating)! > 0 ? "+" : ""}${changeFor(rating)}%` : ""}</span>
                    </li>
                  ))}
              </ol>
            </div>
          ) : null}

          {view.awaiting_decision ? null : (
          <footer className="coach-buttons">
            {feedback ? (
              <button type="button" className="secondary-button" data-testid="coach-hint-button" disabled={!humanTurn || view.awaiting_decision} onClick={actions.hint}>
                {labels.hint}
              </button>
            ) : null}
            <button type="button" className="secondary-button" data-testid="coach-take-back" onClick={actions.takeBack}>
              {labels.takeBack}
            </button>
            <button type="button" className="secondary-button" disabled={!humanTurn || view.awaiting_decision} onClick={actions.pass}>
              {labels.pass}
            </button>
            <button type="button" className="secondary-button" data-testid="coach-resign" onClick={actions.resign}>
              {labels.resign}
            </button>
            <button type="button" className="danger-link" data-testid="coach-end" onClick={actions.end}>
              {labels.end}
            </button>
          </footer>
          )}
        </>
      )}
    </section>
  );
}
