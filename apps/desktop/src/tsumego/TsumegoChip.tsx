import type { ColorView, TsumegoStatus } from "@/api/backend";

export interface TsumegoChipLabels {
  active: string;
  attacks: string;
  black: string;
  white: string;
  noRoom: string;
  noRoomShort: string;
  exit: string;
  exitHint: string;
}

/**
 * Shown in the toolbar while a framed problem is on the board: who attacks, ko threats Java
 * found no room for, and the way back to the original position.
 */
export function TsumegoChip({ status, labels, onExit }: { status: TsumegoStatus; labels: TsumegoChipLabels; onExit(): void }) {
  const color = (side: ColorView) => (side === "Black" ? labels.black : labels.white);
  const warnings = status.ko_warnings.map((side) => labels.noRoom.replace("{color}", color(side)));
  return (
    <span className="chip tsumego-chip" data-testid="tsumego-chip" title={warnings.join("\n") || undefined}>
      <span className="tsumego-mark" aria-hidden="true" />
      <span className="chip-label">{labels.active}</span>
      {status.attacker ? <strong>{labels.attacks.replace("{color}", color(status.attacker))}</strong> : null}
      {warnings.length > 0 ? (
        <span className="tsumego-warning" data-testid="tsumego-warning">
          {labels.noRoomShort}
        </span>
      ) : null}
      <button type="button" className="tsumego-exit" data-testid="tsumego-exit" title={labels.exitHint} onClick={onExit}>
        {labels.exit}
      </button>
    </span>
  );
}
