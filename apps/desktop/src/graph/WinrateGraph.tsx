import { type PointerEvent, useEffect, useMemo, useRef, useState } from "react";
import type { AnalysisView, GraphView } from "@/api/backend";
import { formatScore, formatWinrate } from "../board/format";
import {
  type GraphLayout,
  analysedRanges,
  areaPaths,
  indexAtX,
  isolatedIndices,
  linePaths,
  liveGraph,
  plotHeight,
  scoreRange,
  xForIndex,
  yForScore,
  yForWinrate,
} from "./scale";

export interface GraphLabels {
  label: string;
  empty: string;
  move: string;
}

export interface WinrateGraphProps {
  graph: GraphView;
  analysis: AnalysisView | null;
  labels: GraphLabels;
  onJump(nodeId: number): void;
}

const MIN_HEIGHT = 96;
const PADDING = { left: 40, right: 40, top: 22, bottom: 14 };
/** Side-column padding: the axis labels are short, the plot needs the width. */
const COMPACT_PADDING = { left: 28, right: 30, top: 22, bottom: 14 };

/** Black's winrate (filled) and score lead (thin line) along the displayed line; fills its container. */
export function WinrateGraph({ graph, analysis, labels, onJump }: WinrateGraphProps) {
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

  const points = useMemo(() => liveGraph(graph, analysis), [graph, analysis]);
  const width = size.width;
  const height = Math.max(MIN_HEIGHT, size.height);
  const padding = width < 400 ? COMPACT_PADDING : PADDING;
  const layout: GraphLayout = { width, height, ...padding, count: points.length };
  const range = scoreRange(points);
  const current = points[graph.current_index];
  const analysed = analysedRanges(points);
  const plotBottom = layout.top + plotHeight(layout);

  const jumpTo = (event: PointerEvent<SVGSVGElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const index = indexAtX(layout, event.clientX - rect.left);
    if (index === dragging.current) return;
    dragging.current = index;
    const point = points[index];
    if (point && index !== graph.current_index) onJump(point.node_id);
  };

  const currentLabel = current
    ? [
        labels.move.replace("{n}", String(current.move_number)),
        current.summary ? `${formatWinrate(current.summary.winrate)}%` : null,
        current.summary ? formatScore(current.summary.score_lead) : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : "";

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
          <defs>
            {/* Referenced from CSS (--graph-black-area) so themes can swap it for a flat fill. */}
            <linearGradient id="graph-area-fill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#ecebe6" stopOpacity="0.2" />
              <stop offset="100%" stopColor="#ecebe6" stopOpacity="0.01" />
            </linearGradient>
          </defs>
          <rect className="graph-plot" x={layout.left} y={layout.top} width={width - layout.left - layout.right} height={plotHeight(layout)} rx={4} />
          {[0.25, 0.5, 0.75].map((level) => (
            <line
              key={level}
              className={level === 0.5 ? "graph-grid middle" : "graph-grid"}
              x1={layout.left}
              x2={width - layout.right}
              y1={yForWinrate(layout, level)}
              y2={yForWinrate(layout, level)}
            />
          ))}
          {areaPaths(layout, points).map((path) => (
            <path key={path} className="graph-area" d={path} />
          ))}
          {linePaths(layout, points, (summary) => yForScore(layout, summary.score_lead, range)).map((path) => (
            <path key={path} className="graph-score" d={path} />
          ))}
          {linePaths(layout, points, (summary) => yForWinrate(layout, summary.winrate)).map((path) => (
            <path key={path} className="graph-winrate" d={path} />
          ))}
          {isolatedIndices(points).map((index) => {
            const summary = points[index]?.summary;
            if (!summary) return null;
            const x = xForIndex(layout, index);
            return (
              <g key={index} className="graph-dot" data-testid="graph-dot" data-index={index}>
                <circle className="graph-dot-score" cx={x} cy={yForScore(layout, summary.score_lead, range)} r={2} />
                <circle className="graph-dot-winrate" cx={x} cy={yForWinrate(layout, summary.winrate)} r={2.5} />
              </g>
            );
          })}
          {analysed.map(([from, to]) => (
            <rect
              key={from}
              className="graph-coverage"
              x={xForIndex(layout, from) - 1.5}
              y={plotBottom + 5}
              width={Math.max(3, xForIndex(layout, to) - xForIndex(layout, from) + 3)}
              height={3}
              rx={1.5}
            />
          ))}
          {current ? (
            <g className="graph-current">
              <line x1={xForIndex(layout, graph.current_index)} x2={xForIndex(layout, graph.current_index)} y1={layout.top} y2={plotBottom} />
              {current.summary ? (
                <circle cx={xForIndex(layout, graph.current_index)} cy={yForWinrate(layout, current.summary.winrate)} r={3.5} />
              ) : null}
            </g>
          ) : null}
          <text className="graph-axis" x={layout.left - 6} y={layout.top + 4} textAnchor="end">100</text>
          <text className="graph-axis" x={layout.left - 6} y={yForWinrate(layout, 0.5) + 4} textAnchor="end">50</text>
          <text className="graph-axis" x={layout.left - 6} y={plotBottom} textAnchor="end">0</text>
          <text className="graph-axis score" x={width - layout.right + 6} y={layout.top + 4}>{`+${range}`}</text>
          <text className="graph-axis score" x={width - layout.right + 6} y={plotBottom}>{`−${range}`}</text>
          <text className="graph-caption" data-testid="graph-current" x={layout.left} y={14}>
            {currentLabel}
          </text>
          {analysed.length === 0 ? (
            <text className="graph-empty" data-testid="graph-empty" x={width / 2} y={layout.top + plotHeight(layout) / 2 + 4} textAnchor="middle">
              {labels.empty}
            </text>
          ) : null}
        </svg>
      ) : null}
    </div>
  );
}
