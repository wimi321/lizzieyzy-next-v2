import type { BoardView, GraphPointView } from "@/api/backend";
import { formatWinrate } from "../board/format";
import { lastMoveChange, leadOf, signed } from "./info";

export interface InfoLabels {
  black: string;
  white: string;
  captures: string;
  blackLeads: string;
  whiteLeads: string;
  even: string;
  lastMove: string;
  noAnalysis: string;
}

/** A player as recorded in the game; null fields fall back to the colour's name. */
export interface PlayerInfo {
  name: string | null;
  rank: string | null;
}

export interface InfoPanelProps {
  board: BoardView | null;
  black: PlayerInfo;
  white: PlayerInfo;
  /** The displayed line with the newest summary of the current node merged in. */
  points: readonly GraphPointView[];
  currentIndex: number;
  labels: InfoLabels;
}

/** The players, then the evaluation: both winrates, the bar, the lead and the last move's change. */
export function InfoPanel({ board, black, white, points, currentIndex, labels }: InfoPanelProps) {
  const summary = points[currentIndex]?.summary ?? null;
  const change = board ? lastMoveChange(points, currentIndex, board.last_move) : null;
  const lead = summary ? leadOf(summary) : null;
  const leadText =
    lead === null
      ? labels.noAnalysis
      : lead.side === "Even"
        ? labels.even
        : (lead.side === "Black" ? labels.blackLeads : labels.whiteLeads).replace("{score}", lead.points.toFixed(1));
  const changeText = change
    ? labels.lastMove
        .replace("{n}", String(change.moveNumber))
        .replace("{winrate}", signed(change.winrate))
        .replace("{score}", signed(change.score))
    : null;
  const trend = change === null ? undefined : change.winrate <= -0.05 ? "worse" : change.winrate >= 0.05 ? "better" : "even";
  const blackShare = summary ? summary.winrate : 0.5;

  return (
    <div className="info-panel" data-testid="info-panel">
      <section className="section players">
        <Player
          color="black"
          name={black.name || labels.black}
          rank={black.rank}
          captures={labels.captures.replace("{n}", String(board?.captured_black ?? 0))}
          toPlay={board?.to_play === "Black"}
        />
        <Player
          color="white"
          name={white.name || labels.white}
          rank={white.rank}
          captures={labels.captures.replace("{n}", String(board?.captured_white ?? 0))}
          toPlay={board?.to_play === "White"}
        />
      </section>
      <section className="section evaluation">
        <div className="eval-numbers">
          <span className="eval-number black">
            {summary ? formatWinrate(blackShare) : "—"}
            <small>%</small>
          </span>
          <span className="eval-number white">
            {summary ? formatWinrate(1 - blackShare) : "—"}
            <small>%</small>
          </span>
        </div>
        <div className="winrate-bar" data-testid="winrate-bar" data-black={summary ? formatWinrate(blackShare) : ""}>
          <div className="winrate-black" style={{ width: `${blackShare * 100}%` }} />
        </div>
        <p className="eval-meta" data-testid="lead-line">
          <span className="lead">{leadText}</span>
          {changeText ? (
            <span className="move-change" data-trend={trend}>
              {changeText}
            </span>
          ) : null}
        </p>
      </section>
    </div>
  );
}

function Player(props: { color: "black" | "white"; name: string; rank: string | null; captures: string; toPlay: boolean }) {
  return (
    <div className="player" data-to-play={props.toPlay}>
      <span className={`stone-icon ${props.color}`} aria-hidden="true" />
      <span className="player-name">
        {props.name}
        {props.rank ? <span className="player-rank">{props.rank}</span> : null}
      </span>
      <span className="player-captures">{props.captures}</span>
    </div>
  );
}
