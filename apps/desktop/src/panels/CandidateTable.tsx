import type { AnalysisView, ColorView, PointView } from "@/api/backend";
import { visibleCandidates } from "../board/draw";
import { candidateTier, formatScore, formatVisits, formatWinrate, forMover, winrateLoss } from "../board/format";

export interface TableLabels {
  label: string;
  order: string;
  move: string;
  winrate: string;
  visits: string;
  share: string;
  score: string;
  empty: string;
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

/**
 * The candidate list, in the board's colours: a rank badge per row and a faint
 * bar behind it for the candidate's share of the search.
 */
export function CandidateTable({ analysis, toPlay, maxCandidates, labels, onPreview, onPlay }: CandidateTableProps) {
  const rows = analysis && toPlay ? visibleCandidates(analysis, maxCandidates) : [];
  const total = analysis ? analysis.candidates.reduce((sum, candidate) => sum + candidate.visits, 0) : 0;
  const best = rows[0];
  return (
    <section className="section candidates" aria-label={labels.label} data-testid="candidate-table">
      <div className="section-header">
        <h2 className="section-title">{labels.label}</h2>
        {analysis ? <span className="section-meta">{formatVisits(analysis.visits)}</span> : null}
      </div>
      <div className="candidate-head" aria-hidden="true">
        <span>{labels.order}</span>
        <span>{labels.move}</span>
        <span>{labels.winrate}</span>
        <span>{labels.visits}</span>
        <span>{labels.score}</span>
      </div>
      {toPlay && best ? (
        <ol className="candidate-list" onMouseLeave={() => onPreview(null)}>
          {rows.map((candidate) => {
            const share = total > 0 ? (candidate.visits / total) * 100 : 0;
            const tier = candidateTier(winrateLoss(candidate, best, toPlay), candidate === best);
            return (
              <li key={candidate.coordinate}>
                <button
                  type="button"
                  className="candidate-row"
                  title={`${candidate.coordinate} · ${labels.share} ${share.toFixed(1)}%`}
                  data-testid="candidate-row"
                  data-best={candidate === best}
                  onMouseEnter={() => onPreview(candidate.point)}
                  onFocus={() => onPreview(candidate.point)}
                  onClick={() => candidate.point && onPlay(candidate.point)}
                >
                  <span className="candidate-share" style={{ width: `${share}%` }} aria-hidden="true" />
                  <span className="candidate-rank" style={{ background: `var(--candidate-tier-${Math.min(tier, 5)})` }}>
                    {candidate.order + 1}
                  </span>
                  <span className="candidate-move">{candidate.coordinate}</span>
                  <span className="candidate-winrate">{formatWinrate(forMover(candidate.winrate, toPlay, "winrate"))}</span>
                  <span>{formatVisits(candidate.visits)}</span>
                  <span>{formatScore(forMover(candidate.score_lead, toPlay, "score"))}</span>
                </button>
              </li>
            );
          })}
        </ol>
      ) : (
        <p className="candidates-empty">{labels.empty}</p>
      )}
    </section>
  );
}
