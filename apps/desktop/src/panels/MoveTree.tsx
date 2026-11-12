import { useEffect, useMemo, useRef } from "react";
import type { MoveRank, VariationTreeView } from "@/api/backend";
import { columnLabel, rowLabel } from "../board/format";
import { type PlacedNode, layoutTree } from "./treeLayout";

export interface MoveTreeProps {
  tree: VariationTreeView | null;
  current: number;
  /** Node ids on the displayed line (root to current, then first children). */
  line: readonly number[];
  boardSize: number;
  /** How good each ranked move on the displayed line was, by node id; a dot takes its rank's colour. */
  ranks?: ReadonlyMap<number, MoveRank>;
  label: string;
  onJump(nodeId: number): void;
}

const ROW = 18;
const LANE = 18;
const GUTTER = 30;
const TOP = 12;

/**
 * The game tree as LizzieYzy Next draws it: one dot a move down the main line, variations
 * branching right, the displayed line in white, each dot in the colour of its move's rank.
 */
export function MoveTree({ tree, current, line, boardSize, ranks, label, onJump }: MoveTreeProps) {
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
    <div className="tree-panel" ref={scroller} role="group" aria-label={label} data-testid="move-tree">
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
            const rank = ranks?.get(node.id);
            const isCurrent = node.id === current;
            const coordinate =
              node.mv?.type === "Play" ? `${columnLabel(node.mv.data.point.x, boardSize)}${rowLabel(node.mv.data.point.y, boardSize)}` : node.mv ? "pass" : "";
            const numbered = node.move_number > 0 && (isCurrent || (placed.lane === 0 && node.move_number % 10 === 0));
            const dot = `tree-dot${node.mv ? "" : " empty"}${onLine.has(node.id) ? "" : " off-line"}`;
            return (
              <g
                key={node.id}
                className="tree-node"
                data-testid="tree-node"
                data-id={node.id}
                data-current={isCurrent}
                data-rank={rank}
                data-comment={node.has_comment || undefined}
                onClick={() => !isCurrent && onJump(node.id)}
              >
                <title>{coordinate ? `${node.move_number} · ${coordinate}` : String(node.move_number)}</title>
                {numbered ? (
                  <text className={isCurrent ? "tree-number current" : "tree-number"} x={x(placed) - 12} y={y(placed) + 3.5} textAnchor="end">
                    {node.move_number}
                  </text>
                ) : null}
                {/* Java rings a commented move in blue. */}
                {node.has_comment ? <circle className="tree-comment" data-testid="tree-comment" cx={x(placed)} cy={y(placed)} r={7.2} /> : null}
                {isCurrent ? <circle className="tree-current" cx={x(placed)} cy={y(placed)} r={node.has_comment ? 9.6 : 8} /> : null}
                {node.parent === null ? (
                  <rect className="tree-root" x={x(placed) - 4.5} y={y(placed) - 4.5} width={9} height={9} rx={2} />
                ) : (
                  <circle
                    className={dot}
                    cx={x(placed)}
                    cy={y(placed)}
                    r={node.mv?.type === "Play" ? 5 : 3.5}
                    style={rank ? { fill: `var(--rank-${rank.toLowerCase()})` } : undefined}
                  />
                )}
                {/* A wider transparent target makes the small dots easy to click. */}
                <circle cx={x(placed)} cy={y(placed)} r={9} fill="transparent" />
              </g>
            );
          })}
        </svg>
      ) : null}
    </div>
  );
}
