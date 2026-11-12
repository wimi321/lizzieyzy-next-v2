import type { AnalysisSummary, GraphPointView, MoveView } from "@/api/backend";
import { playoutsText } from "../frame/title";

/** Change caused by the last move, from the mover's side: winrate in percentage points, score in points. */
export interface MoveChange {
  moveNumber: number;
  winrate: number;
  score: number;
}

/**
 * Compares the current node with its parent on the displayed line. Summaries
 * are Black's perspective; the change is flipped for a White move so that a
 * negative number always means the move lost ground, as in LizzieYzy Next.
 */
export function lastMoveChange(
  points: readonly GraphPointView[],
  currentIndex: number,
  lastMove: MoveView | null,
): MoveChange | null {
  const current = points[currentIndex];
  const previous = points[currentIndex - 1]?.summary;
  if (!current?.summary || !previous || !lastMove) return null;
  const sign = lastMove.data.color === "Black" ? 1 : -1;
  return {
    moveNumber: current.move_number,
    winrate: sign * (current.summary.winrate - previous.winrate) * 100,
    score: sign * (current.summary.score_lead - previous.score_lead),
  };
}

export type Lead = { side: "Black" | "White"; points: number } | { side: "Even" };

/** Who leads by how much, rounded the way it is shown. */
export function leadOf(summary: AnalysisSummary): Lead {
  const points = Math.round(Math.abs(summary.score_lead) * 10) / 10;
  if (points === 0) return { side: "Even" };
  return { side: summary.score_lead > 0 ? "Black" : "White", points };
}

/** Signed, one decimal, with a real minus sign: "+0.3", "−1.2", "0.0". */
export function signed(value: number): string {
  const rounded = Math.round(value * 10) / 10;
  if (rounded === 0) return "0.0";
  return `${rounded > 0 ? "+" : "−"}${Math.abs(rounded).toFixed(1)}`;
}

export interface SummaryLabels {
  blackSide: string;
  whiteSide: string;
  /** "{side} 胜率: {winrate}%" */
  winrate: string;
  /** "领先: {lead}" */
  lead: string;
  /** "({engine} / {visits} 计算量)" */
  engine: string;
  /** "({visits} 计算量)", before the engine has a name. */
  visits: string;
  /** "贴目: {komi}" */
  komi: string;
}

/**
 * LizzieYzy Next's note on an analysed move, from the side that played it: its winrate and its
 * lead, each with the change the move made, then the engine with its visits, then the komi.
 * The root, which nobody played, is read from Black's side.
 */
export function analysisSummary(
  points: readonly GraphPointView[],
  currentIndex: number,
  lastMove: MoveView | null,
  engine: string | null,
  komi: number,
  labels: SummaryLabels,
): string[] {
  const summary = points[currentIndex]?.summary;
  if (!summary) return [];
  const black = lastMove === null || lastMove.data.color === "Black";
  const change = lastMoveChange(points, currentIndex, lastMove);
  const winrate = (black ? summary.winrate : 1 - summary.winrate) * 100;
  const lead = Math.round((black ? summary.score_lead : -summary.score_lead) * 10) / 10;
  const visits = playoutsText(summary.visits);
  return [
    labels.winrate.replace("{side}", black ? labels.blackSide : labels.whiteSide).replace("{winrate}", winrate.toFixed(1)) +
      (change ? ` (${signed(change.winrate)}%)` : ""),
    labels.lead.replace("{lead}", lead === 0 ? "0.0" : lead.toFixed(1).replace("-", "−")) + (change ? ` (${signed(change.score)})` : ""),
    engine ? labels.engine.replace("{engine}", engine).replace("{visits}", visits) : labels.visits.replace("{visits}", visits),
    labels.komi.replace("{komi}", String(komi)),
  ];
}
