import type { BoardView, GameInfoView, GraphPointView } from "@/api/backend";
import blackStone from "../assets/java/black0.png?no-inline";
import whiteStone from "../assets/java/white0.png?no-inline";
import { formatWinrate } from "../board/format";
import { lastMoveChange, leadOf, signed } from "./info";

export interface InfoLabels {
  black: string;
  white: string;
  blackShort: string;
  whiteShort: string;
  blackToPlay: string;
  whiteToPlay: string;
  captures: string;
  komi: string;
  handicap: string;
  match: string;
  matchHint: string;
  blackLeads: string;
  whiteLeads: string;
  even: string;
  lastMove: string;
  noAnalysis: string;
  /** Names for common RU values, keyed by the lower-case, dash-joined value. */
  rules: Record<string, string>;
}

export interface CapturedBoxProps {
  board: BoardView | null;
  game: GameInfoView | null;
  /** Match rates (0–1) from the strength estimate, once it has enough moves to mean something. */
  blackMatch?: number | null;
  whiteMatch?: number | null;
  labels: InfoLabels;
}

/**
 * LizzieYzy Next's captured-stones box, Black's column left and White's right: a stone each, the
 * larger one to move, around the rules; the match rates around whose turn it is; the captures
 * around the komi.
 */
export function CapturedBox({ board, game, blackMatch = null, whiteMatch = null, labels }: CapturedBoxProps) {
  const blackToPlay = board?.to_play !== "White";
  return (
    <div className="captured-box" data-testid="captured-box">
      <img
        className="cap-stone"
        src={blackStone}
        alt={labels.black}
        title={player(game?.black_name, game?.black_rank, labels.black)}
        data-to-play={board ? blackToPlay : undefined}
      />
      <span className="cap-rules" data-testid="game-rules" title={game ? details(game, labels) : undefined}>
        {game ? rulesName(game, labels) : null}
      </span>
      <img
        className="cap-stone"
        src={whiteStone}
        alt={labels.white}
        title={player(game?.white_name, game?.white_rank, labels.white)}
        data-to-play={board ? !blackToPlay : undefined}
      />
      <Match side="black" value={blackMatch} labels={labels} />
      <span className="cap-turn" data-testid="to-play">
        {board ? (
          <>
            <i aria-hidden="true" />
            {blackToPlay ? labels.blackToPlay : labels.whiteToPlay}
          </>
        ) : null}
      </span>
      <Match side="white" value={whiteMatch} labels={labels} />
      <span>{labels.captures.replace("{n}", String(board?.captured_black ?? 0))}</span>
      <span data-testid="game-komi">{game ? labels.komi.replace("{n}", String(game.komi)) : null}</span>
      <span>{labels.captures.replace("{n}", String(board?.captured_white ?? 0))}</span>
    </div>
  );
}

function Match({ side, value, labels }: { side: "black" | "white"; value: number | null; labels: InfoLabels }) {
  if (value === null) return <span />;
  return (
    <span className="cap-match" title={labels.matchHint} data-testid={`player-match-${side}`}>
      {labels.match.replace("{n}", (value * 100).toFixed(1))}
    </span>
  );
}

function player(name: string | null | undefined, rank: string | null | undefined, fallback: string): string {
  return [name?.trim() || fallback, rank?.trim()].filter(Boolean).join(" ");
}

/** RU as recorded, with the common names translated; a handicap game without RU says so instead. */
function rulesName(game: GameInfoView, labels: InfoLabels): string {
  const rules = game.rules?.trim();
  if (rules) return labels.rules[rules.toLowerCase().replace(/[\s_]+/g, "-")] ?? rules;
  return game.handicap !== null && game.handicap >= 2 ? labels.handicap.replace("{n}", String(game.handicap)) : "";
}

/** What the box has no room for, on hover: handicap, result, date and event. */
function details(game: GameInfoView, labels: InfoLabels): string | undefined {
  const parts = [
    game.handicap !== null && game.handicap >= 2 ? labels.handicap.replace("{n}", String(game.handicap)) : null,
    game.result?.trim(),
    game.date?.trim(),
    game.event?.trim(),
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : undefined;
}

export interface StatisticsBoxProps {
  board: BoardView | null;
  /** The displayed line with the newest summary of the current node merged in. */
  points: readonly GraphPointView[];
  currentIndex: number;
  labels: InfoLabels;
}

/** LizzieYzy Next's statistics: both winrates around the move number, the bar, then the lead and the last move's change. */
export function StatisticsBox({ board, points, currentIndex, labels }: StatisticsBoxProps) {
  const summary = points[currentIndex]?.summary ?? null;
  // Java's old LZ data may carry a winrate without a score.
  const scoreKnown = summary?.score_lead_available !== false && points[currentIndex - 1]?.summary?.score_lead_available !== false;
  const change = board && scoreKnown ? lastMoveChange(points, currentIndex, board.last_move) : null;
  const lead = summary && summary.score_lead_available !== false ? leadOf(summary) : null;
  const leadText =
    lead === null
      ? summary
        ? ""
        : labels.noAnalysis
      : lead.side === "Even"
        ? labels.even
        : (lead.side === "Black" ? labels.blackLeads : labels.whiteLeads).replace("{score}", lead.points.toFixed(1));
  const changeText = change ? labels.lastMove.replace("{winrate}", signed(change.winrate)).replace("{score}", signed(change.score)) : null;
  const trend = change === null ? undefined : change.winrate <= -0.05 ? "worse" : change.winrate >= 0.05 ? "better" : "even";
  const blackShare = summary ? summary.winrate : 0.5;

  return (
    <div className="stat-box">
      <div className="stat-numbers">
        <span>{`${labels.blackShort} ${summary ? `${formatWinrate(blackShare)}%` : "—"}`}</span>
        <strong data-testid="move-number">{board ? board.move_number : "—"}</strong>
        <span>{`${labels.whiteShort} ${summary ? `${formatWinrate(1 - blackShare)}%` : "—"}`}</span>
      </div>
      <div className="stat-bar" data-testid="winrate-bar" data-black={summary ? formatWinrate(blackShare) : ""}>
        <div className="stat-bar-black" style={{ width: `${blackShare * 100}%` }} />
      </div>
      <p className="stat-lead" data-testid="lead-line">
        <span>{leadText}</span>
        {changeText ? (
          <span className="move-change" data-trend={trend}>
            {changeText}
          </span>
        ) : null}
      </p>
    </div>
  );
}
