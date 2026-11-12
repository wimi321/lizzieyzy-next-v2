import { type PointerEvent, useEffect, useMemo, useRef, useState } from "react";
import type { AnalysisSummary, AnalysisView, GraphView, MistakeBarPoint, MoveRank, WinrateGraphMode } from "@/api/backend";
import { formatWinrate } from "../board/format";
import { shiftScores } from "../display/settings";
import {
  areaPaths,
  type GraphLayout,
  analysedRanges,
  indexAtX,
  isolatedIndices,
  linePaths,
  liveGraph,
  moveTicks,
  plotHeight,
  plotWidth,
  scoreRange,
  xForIndex,
  yForScore,
  yForWinrate,
} from "./scale";

export interface GraphLabels {
  label: string;
  empty: string;
  legendWinrate: string;
  legendScore: string;
}

export interface WinrateGraphProps {
  graph: GraphView;
  analysis: AnalysisView | null;
  /** Per-point losses on the same line and indexing as the graph. */
  mistakes?: readonly MistakeBarPoint[];
  /** 显示胜率/目数曲线. */
  mode?: WinrateGraphMode;
  /** 单曲线模式使用面积填充: the one line is filled to the middle. */
  fill?: boolean;
  /** 显示柱状失误条. */
  bars?: boolean;
  /** Added to Black's lead: komi when 目数显示 shows the board score instead of KataGo's lead. */
  scoreOffset?: number;
  labels: GraphLabels;
  onJump(nodeId: number): void;
}

const MIN_HEIGHT = 96;
/** Only real problems get a bar, as in LizzieYzy Next's blunder bars. */
const BAR_RANKS = new Set<MoveRank>(["Inaccuracy", "Mistake", "Blunder"]);
/** The plot runs edge to edge as in Java; the legend sits above it, the move numbers below. */
const PADDING = { left: 4, right: 4, top: 18, bottom: 15 };

/** "B+1.6" or "W+5.2", as Java labels the score line at the current move. */
function leadText(score: number): string {
  const rounded = Math.round(Math.abs(score) * 10) / 10;
  return `${score >= 0 ? "B" : "W"}+${rounded.toFixed(1)}`;
}

/**
 * LizzieYzy Next's winrate graph: Black's winrate in blue and the score lead in purple along the
 * displayed line, the current move marked with both values. It fills its container; a click or a
 * drag jumps to the move under the pointer.
 */
export function WinrateGraph({ graph, analysis, mistakes, mode = "Both", fill = false, bars = true, scoreOffset = 0, labels, onJump }: WinrateGraphProps) {
  const container = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const dragging = useRef<number | null>(null);

  useEffect(() => {
    const element = container.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    // client sizes exclude the border, so the SVG never pushes its own container larger.
    const observer = new ResizeObserver(() =>
      setSize((previous) =>
        previous.width === element.clientWidth && previous.height === element.clientHeight
          ? previous
          : { width: element.clientWidth, height: element.clientHeight },
      ),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const points = useMemo(() => shiftScores(liveGraph(graph, analysis), scoreOffset), [graph, analysis, scoreOffset]);
  const width = size.width;
  const height = Math.max(MIN_HEIGHT, size.height);
  const layout: GraphLayout = { width, height, ...PADDING, count: points.length };
  const range = scoreRange(points);
  const current = points[graph.current_index];
  const analysed = analysedRanges(points);
  const plotTop = layout.top;
  const plotBottom = layout.top + plotHeight(layout);
  const plotRight = width - layout.right;
  const showWinrate = mode !== "ScoreLead";
  const showScore = mode !== "Winrate";
  const scoreAt = (summary: AnalysisSummary) => yForScore(layout, summary.score_lead, range);
  const winrateAt = (summary: AnalysisSummary) => yForWinrate(layout, summary.winrate);
  const middle = yForWinrate(layout, 0.5);

  const jumpTo = (event: PointerEvent<SVGSVGElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const index = indexAtX(layout, event.clientX - rect.left);
    if (index === dragging.current) return;
    dragging.current = index;
    const point = points[index];
    if (point && index !== graph.current_index) onJump(point.node_id);
  };

  // The two values at the current move sit above their points, below when the top is too near,
  // and apart from each other.
  const currentX = xForIndex(layout, graph.current_index);
  // Kept clear of the axis labels at either edge.
  const labelX = Math.min(plotRight - 52, Math.max(layout.left + 52, currentX));
  const beside = (y: number) => (y - 18 < plotTop ? y + 16 : y - 7);
  const winrateY = current?.summary ? yForWinrate(layout, current.summary.winrate) : 0;
  const scoreY = current?.summary ? yForScore(layout, current.summary.score_lead, range) : 0;
  const winrateLabelY = beside(winrateY);
  let scoreLabelY = beside(scoreY);
  if (Math.abs(scoreLabelY - winrateLabelY) < 13) {
    scoreLabelY = winrateLabelY + (winrateLabelY + 13 > plotBottom - 2 ? -13 : 13);
  }

  return (
    <div
      ref={container}
      className="graph"
      data-testid="winrate-graph"
      data-count={points.length}
      data-current-index={graph.current_index}
      data-plot-left={layout.left}
      data-plot-right={layout.right}
    >
      {width > 0 ? (
        <svg
          width={width}
          height={height}
          role="img"
          aria-label={labels.label}
          onPointerDown={(event) => {
            event.currentTarget.setPointerCapture?.(event.pointerId);
            dragging.current = null;
            jumpTo(event);
          }}
          onPointerMove={(event) => {
            if (event.buttons & 1) jumpTo(event);
          }}
          onPointerUp={() => {
            dragging.current = null;
          }}
        >
          <rect className="graph-plot" x={layout.left} y={plotTop} width={plotWidth(layout)} height={plotHeight(layout)} />
          {[0.25, 0.5, 0.75].map((level) => (
            <line
              key={level}
              className={level === 0.5 ? "graph-grid middle" : "graph-grid"}
              x1={layout.left}
              x2={plotRight}
              y1={yForWinrate(layout, level)}
              y2={yForWinrate(layout, level)}
            />
          ))}
          {fill && mode !== "Both"
            ? areaPaths(layout, points, showWinrate ? winrateAt : scoreAt, middle).map((path) => (
                <path key={path} className={showWinrate ? "graph-fill winrate" : "graph-fill score"} data-testid="graph-fill" d={path} />
              ))
            : null}
          {(bars ? mistakes : undefined)?.map((point, index) => {
            const loss = point.loss;
            if (!loss || !BAR_RANKS.has(loss.rank)) return null;
            const barWidth = Math.min(6, Math.max(2, (plotWidth(layout) / Math.max(1, points.length)) * 0.7));
            // A 30-point drop fills 40% of the plot; small drops stay visible.
            const barHeight = Math.max(3, Math.min(1, loss.winrate_loss / 30) * plotHeight(layout) * 0.4);
            return (
              <rect
                key={point.node_id}
                className={`graph-mistake ${loss.rank}`}
                data-testid="graph-mistake"
                x={xForIndex(layout, index) - barWidth / 2}
                y={plotBottom - barHeight}
                width={barWidth}
                height={barHeight}
                rx={1}
              />
            );
          })}
          {(showScore ? linePaths(layout, points, scoreAt) : []).map((path) => (
            <path key={path} className="graph-score" data-testid="graph-score" d={path} />
          ))}
          {(showWinrate ? linePaths(layout, points, winrateAt) : []).map((path) => (
            <path key={path} className="graph-winrate" data-testid="graph-winrate" d={path} />
          ))}
          {isolatedIndices(points).map((index) => {
            const summary = points[index]?.summary;
            if (!summary) return null;
            const x = xForIndex(layout, index);
            return (
              <g key={index} className="graph-dot" data-testid="graph-dot" data-index={index}>
                {showScore ? <circle className="graph-dot-score" cx={x} cy={scoreAt(summary)} r={2} /> : null}
                {showWinrate ? <circle className="graph-dot-winrate" cx={x} cy={winrateAt(summary)} r={2.5} /> : null}
              </g>
            );
          })}

          {showWinrate ? (
            <>
              <text className="graph-axis" x={layout.left + 3} y={plotTop + 11}>100</text>
              <text className="graph-axis" x={layout.left + 3} y={middle - 3}>50%</text>
              <text className="graph-axis" x={layout.left + 3} y={plotBottom - 3}>0</text>
            </>
          ) : null}
          {showScore ? (
            <>
              <text className="graph-axis score" x={plotRight - 3} y={plotTop + 11} textAnchor="end">{`+${range}`}</text>
              <text className="graph-axis score" x={plotRight - 3} y={plotBottom - 3} textAnchor="end">{`−${range}`}</text>
            </>
          ) : null}
          {moveTicks(points.length).map((move) => (
            <text key={move} className="graph-axis move" x={xForIndex(layout, move)} y={height - 3} textAnchor="middle">
              {move}
            </text>
          ))}
          <text className="graph-legend" x={plotRight} y={12} textAnchor="end" aria-hidden="true">
            {showWinrate ? (
              <>
                <tspan className="winrate">━ </tspan>
                {labels.legendWinrate}
              </>
            ) : null}
            {showScore ? (
              <>
                <tspan className="score" dx={showWinrate ? 10 : 0}>━ </tspan>
                {labels.legendScore}
              </>
            ) : null}
          </text>

          {current ? (
            <g className="graph-current" data-testid="graph-current">
              <line x1={currentX} x2={currentX} y1={plotTop} y2={plotBottom} />
              {current.summary ? (
                <>
                  {/* Both dots first, then both values, so no dot covers a value. */}
                  {showScore ? <circle cx={currentX} cy={scoreY} r={2.5} className="score" /> : null}
                  {showWinrate ? <circle cx={currentX} cy={winrateY} r={3.5} /> : null}
                  {showScore ? (
                    <text className="graph-value score" x={labelX} y={showWinrate ? scoreLabelY : beside(scoreY)} textAnchor="middle">
                      {leadText(current.summary.score_lead)}
                    </text>
                  ) : null}
                  {showWinrate ? (
                    <text className="graph-value" x={labelX} y={winrateLabelY} textAnchor="middle">
                      {formatWinrate(current.summary.winrate)}
                    </text>
                  ) : null}
                </>
              ) : null}
            </g>
          ) : null}
          {analysed.length === 0 ? (
            <text className="graph-empty" data-testid="graph-empty" x={width / 2} y={plotTop + plotHeight(layout) / 2 + 4} textAnchor="middle">
              {labels.empty}
            </text>
          ) : null}
        </svg>
      ) : null}
    </div>
  );
}
