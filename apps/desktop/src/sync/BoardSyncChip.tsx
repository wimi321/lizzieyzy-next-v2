import type { BoardSyncError, BoardSyncStatus } from "@/api/backend";

export interface BoardSyncLabels {
  starting: string;
  ready: string;
  syncing: string;
  move: string;
  failed: string;
  stop: string;
  stopHint: string;
  close: string;
  unsupported: string;
  missing: string;
}

/** The reason a synchronization stopped, in the user's words. */
function syncError(error: BoardSyncError, labels: BoardSyncLabels): string {
  switch (error.type) {
    case "UnsupportedPlatform":
      return labels.unsupported;
    case "MissingExecutable":
      return labels.missing.replace("{path}", error.data.path);
    case "InvalidOptions":
    case "Process":
    case "Protocol":
      return error.data.reason;
  }
}

/**
 * Shown in the toolbar while readboard runs, as Java's board follows the client: what the tool is
 * doing, the move it last read, and the way to stop it. A failure stays until it is dismissed.
 */
export function BoardSyncChip({ status, labels, onStop }: { status: BoardSyncStatus; labels: BoardSyncLabels; onStop(): void }) {
  if (status.phase === "Stopped") return null;
  const failed = status.phase === "Failed";
  const reason = failed && status.error ? syncError(status.error, labels) : undefined;
  const text =
    status.phase === "Starting"
      ? labels.starting
      : status.phase === "Ready"
        ? labels.ready
        : status.phase === "Syncing"
          ? labels.syncing
          : labels.failed;
  return (
    <span className="chip sync-chip" data-testid="sync-chip" data-phase={status.phase} title={reason}>
      <span className="sync-dot" aria-hidden="true" />
      <span className="chip-label">{text}</span>
      {status.phase === "Syncing" && status.remote_move_number !== null ? (
        <strong data-testid="sync-move">{labels.move.replace("{n}", String(status.remote_move_number))}</strong>
      ) : null}
      <button type="button" className="sync-stop" data-testid="sync-stop" title={failed ? undefined : labels.stopHint} onClick={onStop}>
        {failed ? labels.close : labels.stop}
      </button>
    </span>
  );
}
