import { useEffect, useState } from "react";
import type { Backend, DocumentView, VariationTreeView } from "@/api/backend";

/**
 * The game tree, fetched when its structure or its comment marks change, never on plain
 * navigation. `tree` is the last one fetched, for drawing without a gap; `live` is null while
 * it is out of date.
 */
export function useVariationTree(backend: Pick<Backend, "variationTree">, document: DocumentView | null) {
  const [tree, setTree] = useState<VariationTreeView | null>(null);
  const structure = document?.tree.structure_revision;
  const commentMarks = document?.tree.comment_revision ?? 0;
  useEffect(() => {
    if (structure === undefined) return;
    let active = true;
    backend
      .variationTree()
      .then((view) => {
        if (active && view.revision === structure && (view.comment_revision ?? 0) === commentMarks) setTree(view);
      })
      .catch((error: unknown) => console.error("Variation tree failed", error));
    return () => {
      active = false;
    };
  }, [backend, structure, commentMarks]);
  const live = tree && document && tree.revision === document.tree.structure_revision ? tree : null;
  return { tree, live };
}
