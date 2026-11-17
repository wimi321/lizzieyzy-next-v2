import { useState } from "react";
import type { AnalysisView, Backend, BoardView, EngineStatus, PointView } from "@/api/backend";
import { Board } from "../board/Board";
import type { Estimate } from "../board/estimate";
import { formatVisits, formatWinrate } from "../board/format";
import { useVariationPreview } from "../board/useVariationPreview";
import type en from "../locales/en.json";

export type ComparisonLabels = typeof en.comparison;

/** Java separates the parts of each line with a full-width space. */
const GAP = "\u3000";
const fill = (text: string, values: Record<string, string>) => text.replace(/\{(\w+)\}/g, (whole, key: string) => values[key] ?? whole);

export interface ComparisonBoardProps {
  backend: Pick<Backend, "previewVariation">;
  board: BoardView;
  node: number | null;
  revision: number;
  /** The comparison engine's result for this position, or null. */
  analysis: AnalysisView | null;
  coordinates: boolean;
  /** Kata形势 as the main board shows it: the comparison engine's ownership in the same style. */
  estimate: Estimate | null;
  maxCandidates: number;
  /** No comparison engine is chosen yet. */
  unchosen: boolean;
  labels: ComparisonLabels;
  onPlay(point: PointView): void;
}

/**
 * 双引擎模式's right board (Java's `boardRenderer2`): the comparison engine's candidates on the same
 * position; hovering one shows its line on this board only, and a click plays as on the main board.
 */
export function ComparisonBoard({ backend, board, node, revision, analysis, coordinates, estimate, maxCandidates, unchosen, labels, onPlay }: ComparisonBoardProps) {
  const [hover, setHover] = useState<PointView | null>(null);
  const variation = useVariationPreview(backend, node, revision, analysis, hover);
  const ownership = estimate && analysis?.ownership ? { style: estimate.style, values: analysis.ownership } : null;
  return (
    <div className="comparison-board" data-testid="comparison-board" data-candidates={analysis?.candidates.length ?? 0} data-source={analysis?.source ?? ""}>
      <Board
        board={board}
        analysis={analysis}
        coordinates={coordinates}
        estimate={ownership}
        maxCandidates={maxCandidates}
        label={labels.boardLabel}
        variation={variation}
        onHoverCandidate={setHover}
        onPlay={onPlay}
      />
      {unchosen ? (
        <p className="comparison-pick" data-testid="comparison-pick">
          {labels.pick}
        </p>
      ) : null}
    </div>
  );
}

export interface EngineLine {
  name: string | null;
  analysis: AnalysisView | null;
}

function numbers(analysis: AnalysisView | null, labels: ComparisonLabels): string {
  if (!analysis) return `${labels.visits} —${GAP}${labels.winrate} —`;
  return `${labels.visits} ${formatVisits(analysis.visits)}${GAP}${labels.winrate} ${formatWinrate(analysis.winrate)}%`;
}

function comparisonState(status: EngineStatus, pondering: boolean, labels: ComparisonLabels): string {
  switch (status.type) {
    case "Ready":
      return `${labels.pondering} ${pondering ? labels.on : labels.off}`;
    case "Failed":
      return fill(labels.status.Failed, { reason: status.data.reason });
    default:
      return labels.status[status.type];
  }
}

export interface EngineLinesProps {
  main: EngineLine;
  comparison: EngineLine & { status: EngineStatus };
  /** Space pauses both engines: the comparison engine follows the main one's analysis. */
  pondering: boolean;
  labels: ComparisonLabels;
}

/** Java's two lines under each board: the engine, whether it analyses, its visits and win rate. */
export function EngineLines({ main, comparison, pondering, labels }: EngineLinesProps) {
  const chosen = comparison.name !== null;
  return (
    <div className="comparison-lines" role="group" aria-label={labels.linesLabel} data-testid="comparison-lines">
      <div className="comparison-line" data-testid="comparison-main-line">
        <span>
          {labels.mainEngine} {main.name ?? labels.none}{GAP}{labels.pondering} {pondering ? labels.on : labels.off}{GAP}{labels.hotkey}
        </span>
        <span>{numbers(main.analysis, labels)}</span>
      </div>
      <div className="comparison-line" data-testid="comparison-sub-line" data-state={chosen ? comparison.status.type : "None"}>
        <span>
          {labels.subEngine} {comparison.name ?? labels.none}
          {chosen ? `${GAP}${comparisonState(comparison.status, pondering, labels)}` : null}
        </span>
        <span>{numbers(chosen ? comparison.analysis : null, labels)}</span>
      </div>
    </div>
  );
}
