import type { AnalysisView, Backend, BoardView, PointView } from "@/api/backend";
import { Board } from "../board/Board";
import { rankedLines } from "../board/lines";
import { SubBoard } from "../board/SubBoard";
import { useVariationPreview } from "../board/useVariationPreview";
import type en from "../locales/en.json";

type Labels = typeof en.layout.boards;

export interface SideBoardsProps {
  mode: "Thinking" | "MultiView";
  backend: Pick<Backend, "previewVariation">;
  board: BoardView;
  node: number | null;
  revision: number;
  analysis: AnalysisView | null;
  coordinates: boolean;
  labels: Labels;
  onPlay(point: PointView): void;
}

/** 四方图: the first four candidates' lines on four small boards, as Java's `SubBoardRenderer` 1–4. */
function MultiView({ backend, board, node, revision, analysis, labels }: Omit<SideBoardsProps, "mode" | "coordinates" | "onPlay">) {
  const lines = rankedLines(analysis, node);
  const at = (index: number) => lines[index]?.point ?? null;
  // Four fixed calls: hooks may not run in a loop.
  const previews = [
    useVariationPreview(backend, node, revision, analysis, at(0)),
    useVariationPreview(backend, node, revision, analysis, at(1)),
    useVariationPreview(backend, node, revision, analysis, at(2)),
    useVariationPreview(backend, node, revision, analysis, at(3)),
  ];
  return (
    <div className="multi-view" data-testid="multi-view" data-lines={Math.min(lines.length, 4)}>
      {previews.map((preview, index) => (
        <div key={index} className="multi-view-cell">
          <SubBoard board={board} node={node} revision={revision} variation={at(index) ? preview : null} label={labels.candidate.replace("{n}", String(index + 1))} />
          <span className="multi-view-order" aria-hidden="true">
            {index + 1}
          </span>
        </div>
      ))}
    </div>
  );
}

/**
 * The extra square of 思考模式 and 四方图. 思考模式's is the position alone — no candidates, hints
 * or names — to read on by oneself; a click plays there as on the main board.
 */
export function SideBoards({ mode, coordinates, onPlay, ...rest }: SideBoardsProps) {
  if (mode === "MultiView") return <MultiView {...rest} />;
  return (
    <div className="thinking-board" data-testid="thinking-board">
      <Board board={rest.board} analysis={null} coordinates={coordinates} estimate={null} maxCandidates={0} label={rest.labels.thinking} onPlay={onPlay} />
    </div>
  );
}
