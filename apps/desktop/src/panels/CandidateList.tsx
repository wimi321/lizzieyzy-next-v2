import { useMemo } from "react";
import type { AnalysisView, ColorView, PointView } from "@/api/backend";
import { forMover, formatWinrate } from "../board/format";
import { scoreText } from "../board/suggestion";
import { playoutsText } from "../frame/title";

export interface CandidateListLabels {
  label: string;
  order: string;
  move: string;
  winrate: string;
  visits: string;
  share: string;
  score: string;
  empty: string;
}

export interface CandidateListProps {
  /** Analysis of the displayed node only; anything else is stale. */
  analysis: AnalysisView | null;
  toPlay: ColorView;
  /** 总是显示黑方胜率. */
  blackWinrate: boolean;
  /** Added to Black's lead when 目数显示 shows the board score instead of KataGo's lead. */
  scoreOffset: number;
  labels: CandidateListLabels;
  /** The row under the pointer, so the board previews its variation as it does for a hovered candidate. */
  onHover(point: PointView | null): void;
  onPlay(point: PointView): void;
}

/**
 * Java's 选点列表面板 (`LizzieFrame`'s list pane): the engine's candidates in its order, with the
 * winrate, visits, their share of all visits and the lead. A double click plays the move.
 */
export function CandidateList({ analysis, toPlay, blackWinrate, scoreOffset, labels, onHover, onPlay }: CandidateListProps) {
  const rows = useMemo(() => {
    if (!analysis) return [];
    const total = analysis.candidates.reduce((sum, candidate) => sum + candidate.visits, 0);
    return [...analysis.candidates]
      .sort((a, b) => a.order - b.order)
      .map((candidate) => {
        const blackLead = candidate.score_lead + scoreOffset;
        return {
          key: candidate.coordinate,
          point: candidate.point,
          order: candidate.order + 1,
          move: candidate.coordinate,
          winrate: formatWinrate(blackWinrate ? candidate.winrate : forMover(candidate.winrate, toPlay, "winrate")),
          visits: playoutsText(candidate.visits),
          share: total > 0 ? ((candidate.visits / total) * 100).toFixed(1) : "0.0",
          score: scoreText(blackWinrate ? blackLead : forMover(blackLead, toPlay, "score")),
        };
      });
  }, [analysis, toPlay, blackWinrate, scoreOffset]);

  return (
    <div className="candidate-list" role="group" aria-label={labels.label} data-testid="candidate-list" onMouseLeave={() => onHover(null)}>
      <table>
        <thead>
          <tr>
            <th>{labels.order}</th>
            <th>{labels.move}</th>
            <th>{labels.winrate}</th>
            <th>{labels.visits}</th>
            <th>{labels.share}</th>
            <th>{labels.score}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.key}
              data-testid="candidate-row"
              data-best={row.order === 1 || undefined}
              onMouseEnter={() => onHover(row.point)}
              onDoubleClick={() => row.point && onPlay(row.point)}
            >
              <td>{row.order}</td>
              <td>{row.move}</td>
              <td>{row.winrate}</td>
              <td>{row.visits}</td>
              <td>{row.share}</td>
              <td>{row.score}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {rows.length === 0 ? <p className="candidate-empty">{labels.empty}</p> : null}
    </div>
  );
}
