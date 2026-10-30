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

export interface InfoPanelProps {
  board: BoardView | null;
  /** The displayed line with the newest summary of the current node merged in. */
  points: readonly GraphPointView[];
  currentIndex: number;
  labels: InfoLabels;
}

/** Players, captures, the winrate bar and the lead line at the top of the left column. */
export function InfoPanel({ board, points, currentIndex, labels }: InfoPanelProps) {
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
  const black = summary ? summary.winrate : 0.5;

  return (
    <section className="info-panel" data-testid="info-panel">
      <div className="players">
        <Player color="black" name={labels.black} captures={labels.captures.replace("{n}", String(board?.captured_black ?? 0))} toPlay={board?.to_play === "Black"} />
        <Player color="white" name={labels.white} captures={labels.captures.replace("{n}", String(board?.captured_white ?? 0))} toPlay={board?.to_play === "White"} />
      </div>
      <div className="winrate-bar" data-testid="winrate-bar" data-black={summary ? formatWinrate(black) : ""}>
        <div className="winrate-black" style={{ width: `${black * 100}%` }}>
          {summary ? `${formatWinrate(black)}%` : ""}
        </div>
        <div className="winrate-white">{summary ? `${formatWinrate(1 - black)}%` : ""}</div>
      </div>
      <p className="lead-line" data-testid="lead-line">
        <span>{leadText}</span>
        {changeText ? <span className={change && change.winrate < 0 ? "move-change worse" : "move-change"}>{changeText}</span> : null}
      </p>
    </section>
  );
}

function Player({ color, name, captures, toPlay }: { color: "black" | "white"; name: string; captures: string; toPlay: boolean }) {
  return (
    <div className={`player ${color}`} data-to-play={toPlay}>
      <span className={`stone-icon ${color}`} aria-hidden="true" />
      <span className="player-name">{name}</span>
      <span className="player-captures">{captures}</span>
    </div>
  );
}
