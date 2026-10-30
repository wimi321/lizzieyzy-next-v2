import { useEffect, useMemo, useRef } from "react";
import type { VariationTreeView } from "@/api/backend";
import { columnLabel, rowLabel } from "../board/format";
import { type PlacedNode, layoutTree } from "./treeLayout";

export interface MoveTreeLabels {
  label: string;
  variations: string;
}

export interface MoveTreeProps {
  tree: VariationTreeView | null;
  current: number;
  /** Node ids on the displayed line (root to current, then first children). */
  line: readonly number[];
  boardSize: number;
  variationCount: number;
  labels: MoveTreeLabels;
  onJump(nodeId: number): void;
}

const ROW = 18;
const LANE = 18;
const GUTTER = 30;
const TOP = 10;

/** The game tree drawn as LizzieYzy Next draws it: stones down the main line, variations branching right. */
export function MoveTree({ tree, current, line, boardSize, variationCount, labels, onJump }: MoveTreeProps) {
  const layout = useMemo(() => (tree ? layoutTree(tree) : null), [tree]);
  const onLine = useMemo(() => new Set(line), [line]);
  const scroller = useRef<HTMLDivElement>(null);
  const currentNode = layout?.byId.get(current) ?? null;

  useEffect(() => {
    const element = scroller.current;
    if (!element || !currentNode) return;
    const y = TOP + currentNode.row * ROW;
    if (y < element.scrollTop + ROW || y > element.scrollTop + element.clientHeight - ROW * 2) {
      element.scrollTop = Math.max(0, y - element.clientHeight / 2);
    }
  }, [currentNode]);

  const x = (placed: PlacedNode) => GUTTER + placed.lane * LANE + LANE / 2;
  const y = (placed: PlacedNode) => TOP + placed.row * ROW;
  const width = layout ? GUTTER + layout.lanes * LANE + 8 : 0;
  const height = layout ? TOP * 2 + Math.max(0, layout.rows - 1) * ROW : 0;

  return (
    <section className="section tree-section" aria-label={labels.label} data-testid="move-tree">
      <div className="section-header">
        <h2 className="section-title">{labels.label}</h2>
        {variationCount > 1 ? <span className="section-meta">{labels.variations.replace("{n}", String(variationCount))}</span> : null}
      </div>
      <div className="tree-scroll" ref={scroller}>
        {layout ? (
          <svg width={width} height={height} className="tree">
            {layout.edges.map(({ from, to }) => {
              const lit = onLine.has(from.node.id) && onLine.has(to.node.id);
              const d =
                from.lane === to.lane
                  ? `M${x(from)} ${y(from)}V${y(to)}`
                  : `M${x(from)} ${y(from)}C${x(from)} ${y(from) + ROW * 0.7} ${x(to)} ${y(to) - ROW * 0.7} ${x(to)} ${y(to)}`;
              return <path key={to.node.id} d={d} className={lit ? "tree-edge lit" : "tree-edge"} />;
            })}
            {layout.nodes.map((placed) => {
              const { node } = placed;
              const color = node.mv?.data.color;
              const label = node.mv?.type === "Play" ? `${columnLabel(node.mv.data.point.x, boardSize)}${rowLabel(node.mv.data.point.y, boardSize)}` : node.mv ? "pass" : "";
              return (
                <g
                  key={node.id}
                  className="tree-node"
                  data-testid="tree-node"
                  data-id={node.id}
                  data-current={node.id === current}
                  onClick={() => node.id !== current && onJump(node.id)}
                >
                  <title>{label ? `${node.move_number} · ${label}` : String(node.move_number)}</title>
                  {placed.lane === 0 && node.move_number > 0 && node.move_number % 10 === 0 ? (
                    <text className="tree-number" x={GUTTER - 10} y={y(placed) + 3.5} textAnchor="end">
                      {node.move_number}
                    </text>
                  ) : null}
                  {node.id === current ? <circle className="tree-current" cx={x(placed)} cy={y(placed)} r={8.5} /> : null}
                  <circle
                    className={`tree-stone ${color === "Black" ? "black" : color === "White" ? "white" : "empty"}${onLine.has(node.id) ? "" : " off-line"}`}
                    cx={x(placed)}
                    cy={y(placed)}
                    r={color && node.mv?.type === "Play" ? 5 : 3.5}
                  />
                  {/* A wider transparent target makes the small stones easy to click. */}
                  <circle cx={x(placed)} cy={y(placed)} r={9} fill="transparent" />
                </g>
              );
            })}
          </svg>
        ) : null}
      </div>
    </section>
  );
}
