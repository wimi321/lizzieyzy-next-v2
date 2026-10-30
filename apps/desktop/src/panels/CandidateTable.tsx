import type { AnalysisView, ColorView, PointView } from "@/api/backend";
import { visibleCandidates } from "../board/draw";
import { formatScore, formatVisits, formatWinrate, forMover } from "../board/format";

export interface TableLabels {
  label: string;
  order: string;
  winrate: string;
  visits: string;
  share: string;
  score: string;
}

export interface CandidateTableProps {
  analysis: AnalysisView | null;
  toPlay: ColorView | null;
  maxCandidates: number;
  labels: TableLabels;
  /** Hovering a row previews that candidate's variation on the board. */
  onPreview(point: PointView | null): void;
  onPlay(point: PointView): void;
}

/** The candidate list under the move list, with the same numbers as the board labels. */
export function CandidateTable({ analysis, toPlay, maxCandidates, labels, onPreview, onPlay }: CandidateTableProps) {
  const rows = analysis && toPlay ? visibleCandidates(analysis, maxCandidates) : [];
  const total = analysis ? analysis.candidates.reduce((sum, candidate) => sum + candidate.visits, 0) : 0;
  return (
    <section className="candidate-table" aria-label={labels.label} data-testid="candidate-table">
      <table onMouseLeave={() => onPreview(null)}>
        <thead>
          <tr>
            <th>{labels.order}</th>
            <th>{labels.winrate}</th>
            <th>{labels.visits}</th>
            <th>{labels.share}</th>
            <th>{labels.score}</th>
          </tr>
        </thead>
        <tbody>
          {toPlay &&
            rows.map((candidate) => (
              <tr
                key={candidate.coordinate}
                title={candidate.coordinate}
                data-testid="candidate-row"
                data-best={candidate.order === 0}
                onMouseEnter={() => onPreview(candidate.point)}
                onClick={() => candidate.point && onPlay(candidate.point)}
              >
                <td>{candidate.order + 1}</td>
                <td>{formatWinrate(forMover(candidate.winrate, toPlay, "winrate"))}</td>
                <td>{formatVisits(candidate.visits)}</td>
                <td>{total > 0 ? ((candidate.visits / total) * 100).toFixed(1) : "—"}</td>
                <td>{formatScore(forMover(candidate.score_lead, toPlay, "score"))}</td>
              </tr>
            ))}
        </tbody>
      </table>
    </section>
  );
}
