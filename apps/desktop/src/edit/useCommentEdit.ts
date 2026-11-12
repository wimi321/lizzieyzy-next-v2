import { useCallback, useEffect, useRef, useState } from "react";
import type { Backend, DocumentView } from "@/api/backend";

interface Draft {
  node: number;
  /** The tree the move belongs to: another game, or a synchronized board, voids the draft. */
  structure: number;
  original: string;
  text: string;
}

export interface CommentEdit {
  /** The text being edited, or null while the comment is only shown. */
  text: string | null;
  begin(): void;
  change(text: string): void;
  /** Saves a changed text to the move it was written for; does nothing when nothing is being edited. */
  commit(): void;
  cancel(): void;
}

function report(error: unknown) {
  console.error("Saving the comment failed", error);
}

/**
 * Editing the current move's comment as LizzieYzy Next does: click it, type, click elsewhere.
 * The app commits before it navigates by itself; when the game changes under the editor
 * (another game opened, the board synchronized) the unsaved text is dropped, never written
 * to a different move.
 */
export function useCommentEdit(backend: Pick<Backend, "setComment">, document: DocumentView | null): CommentEdit {
  const [draft, setDraft] = useState<Draft | null>(null);
  // Read by commit(), which runs from blur and from navigation within the same event.
  const latest = useRef<Draft | null>(null);
  const put = useCallback((next: Draft | null) => {
    latest.current = next;
    setDraft(next);
  }, []);

  const node = document?.tree.current ?? null;
  const structure = document?.tree.structure_revision ?? null;
  // Where the game is now, for the callbacks: a draft written for another place is void.
  const where = useRef({ node, structure });
  useEffect(() => {
    where.current = { node, structure };
  });
  const live = draft !== null && draft.node === node && draft.structure === structure;

  const begin = useCallback(() => {
    if (!document) return;
    const editing = latest.current;
    if (editing && editing.node === document.tree.current && editing.structure === document.tree.structure_revision) return;
    const original = document.comment ?? "";
    put({ node: document.tree.current, structure: document.tree.structure_revision, original, text: original });
  }, [document, put]);

  const change = useCallback(
    (text: string) => {
      if (latest.current) put({ ...latest.current, text });
    },
    [put],
  );

  const commit = useCallback(() => {
    const current = latest.current;
    if (!current) return;
    put(null);
    if (current.node !== where.current.node || current.structure !== where.current.structure) return;
    // Java drops the line break it adds for typing convenience.
    const text = current.text.replace(/\n+$/, "");
    if (text !== current.original) backend.setComment(text).catch(report);
  }, [backend, put]);

  const cancel = useCallback(() => put(null), [put]);

  return { text: live ? draft.text : null, begin, change, commit, cancel };
}
