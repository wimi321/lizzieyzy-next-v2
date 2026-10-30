import { useEffect, useRef, useState } from "react";
import type { AnalysisView, Backend, MoveView, PointView, VariationPreviewView } from "@/api/backend";

function report(error: unknown) {
  console.error("Variation preview failed", error);
}

/**
 * Rust replays the hovered candidate's principal variation, captures included.
 * Only the newest request counts, and only while its node and revision still
 * match the displayed position; until then the board keeps its own overlay.
 */
export function useVariationPreview(
  backend: Pick<Backend, "previewVariation">,
  node: number | null,
  revision: number,
  analysis: AnalysisView | null,
  target: PointView | null,
): VariationPreviewView | null {
  const [preview, setPreview] = useState<{ key: string; view: VariationPreviewView } | null>(null);
  const latest = useRef(0);

  const candidate =
    target && analysis && node !== null && analysis.node_id === node
      ? analysis.candidates.find((c) => c.point?.x === target.x && c.point?.y === target.y && c.pv.length > 0)
      : undefined;
  // The engine deepens a variation while it searches; a changed line is a new
  // request, while a new analysis frame with the same line is not.
  const line = candidate ? JSON.stringify(candidate.pv) : null;
  const key = line === null || node === null ? null : `${revision}:${node}:${line}`;

  useEffect(() => {
    const request = ++latest.current;
    if (line === null || node === null) return;
    const moves = JSON.parse(line) as MoveView[];
    backend
      .previewVariation(node, moves)
      .then((view) => {
        if (request !== latest.current || view.node_id !== node || view.revision !== revision) return;
        setPreview({ key: `${revision}:${node}:${line}`, view });
      })
      .catch(report);
  }, [backend, node, revision, line]);

  return preview && preview.key === key ? preview.view : null;
}
