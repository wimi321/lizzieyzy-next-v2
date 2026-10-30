import type { ColorView, MoveRank, ProblemMoveSort, ProblemMovesView } from "@/api/backend";

export interface ProblemLabels {
  all: string;
  black: string;
  white: string;
  byLoss: string;
  byMove: string;
  empty: string;
  noAnalysis: string;
  move: string;
  points: string;
  ranks: Record<MoveRank, string>;
}

export interface ProblemListProps {
  view: ProblemMovesView | null;
  current: number;
  color: ColorView | null;
  sort: ProblemMoveSort;
  /** Whether any position on the line has been analysed yet. */
  analysed: boolean;
  labels: ProblemLabels;
  onColor(color: ColorView | null): void;
  onSort(sort: ProblemMoveSort): void;
  onJump(nodeId: number): void;
}

const COLOR_FILTERS: Array<{ key: string; color: ColorView | null }> = [
  { key: "all", color: null },
  { key: "black", color: "Black" },
  { key: "white", color: "White" },
];

/** LizzieYzy Next's problem-move list: worst moves first, filterable by player, one click to jump. */
export function ProblemList({ view, current, color, sort, analysed, labels, onColor, onSort, onJump }: ProblemListProps) {
  const entries = view?.entries ?? [];
  return (
    <div className="problem-list" data-testid="problem-list">
      <div className="problem-toolbar">
        <div className="segmented small" role="radiogroup">
          {COLOR_FILTERS.map((filter) => (
            <button
              key={filter.key}
              type="button"
              role="radio"
              aria-checked={color === filter.color}
              data-testid={`problems-${filter.key}`}
              onClick={() => onColor(filter.color)}
            >
              {labels[filter.key as "all" | "black" | "white"]}
            </button>
          ))}
        </div>
        <select
          className="select compact"
          data-testid="problems-sort"
          value={sort === "LossDesc" ? "LossDesc" : "MoveAsc"}
          onChange={(event) => onSort(event.target.value as ProblemMoveSort)}
        >
          <option value="LossDesc">{labels.byLoss}</option>
          <option value="MoveAsc">{labels.byMove}</option>
        </select>
      </div>
      {entries.length > 0 ? (
        <ol className="problem-rows">
          {entries.map((entry) => (
            <li key={entry.node_id}>
              <button
                type="button"
                className="problem-row"
                data-testid="problem-row"
                data-rank={entry.loss.rank}
                aria-current={entry.node_id === current ? "step" : undefined}
                title={labels.ranks[entry.loss.rank]}
                onClick={() => onJump(entry.node_id)}
              >
                <span className="rank-dot" aria-hidden="true" />
                <span className="problem-number">{labels.move.replace("{n}", String(entry.move_number))}</span>
                <span className={`stone-icon tiny ${entry.color === "Black" ? "black" : "white"}`} aria-hidden="true" />
                <span className="problem-coordinate">{entry.coordinate}</span>
                <span className="problem-loss">−{entry.loss.winrate_loss.toFixed(1)}%</span>
                <span className="problem-score">{labels.points.replace("{n}", `−${entry.loss.score_loss.toFixed(1)}`)}</span>
              </button>
            </li>
          ))}
        </ol>
      ) : (
        <p className="panel-empty">{analysed ? labels.empty : labels.noAnalysis}</p>
      )}
    </div>
  );
}
