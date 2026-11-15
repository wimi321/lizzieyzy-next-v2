import { type PointerEvent, useState } from "react";
import type { ControlPanelBounds } from "@/api/backend";
import { isEnabled, type MenuBinding } from "../menu/bindings";
import type en from "../locales/en.json";
import { type Bounds, placed } from "./values";

type Labels = typeof en.research.panel;

const PANEL_BUTTONS = ["first", "last", "back5", "forward5", "back", "forward", "main", "paint"] as const;
export type PanelButton = (typeof PANEL_BUTTONS)[number];

/** Java's grid, row by row: |< >|, << >>, < >, 返回 涂鸦. */
const TEXT: Record<PanelButton, string | null> = { first: "|<", last: ">|", back5: "<<", forward5: ">>", back: "<", forward: ">", main: null, paint: null };

export interface ControlPanelProps {
  bounds: ControlPanelBounds | null;
  actions: Readonly<Record<PanelButton, MenuBinding | undefined>>;
  labels: Labels;
  onBounds(bounds: Bounds): void;
  onClose(): void;
}

type Drag = { kind: "move" | "size"; x: number; y: number; from: Bounds; now: Bounds };

/**
 * 独立棋局控制面板: a remote with big buttons that can be dragged anywhere and sized, as Java's
 * `Controller` for teaching on a projector. Where it was left is saved for the next time.
 */
export function ControlPanel({ bounds, actions, labels, onBounds, onClose }: ControlPanelProps) {
  const [drag, setDrag] = useState<Drag | null>(null);
  const shown = drag?.now ?? placed(bounds);

  const begin = (kind: Drag["kind"]) => (event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0 || (event.target as Element).closest("button")) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    setDrag({ kind, x: event.clientX, y: event.clientY, from: shown, now: shown });
  };
  const follow = (event: PointerEvent<HTMLElement>) => {
    if (!drag) return;
    const dx = event.clientX - drag.x;
    const dy = event.clientY - drag.y;
    const { from } = drag;
    const now = drag.kind === "move" ? { ...from, x: from.x + dx, y: from.y + dy } : { ...from, width: from.width + dx, height: from.height + dy };
    setDrag({ ...drag, now: placed(now) });
  };
  const finish = () => {
    if (!drag) return;
    setDrag(null);
    const { now, from } = drag;
    if (now.x !== from.x || now.y !== from.y || now.width !== from.width || now.height !== from.height) onBounds(now);
  };

  return (
    <section
      className="control-panel"
      role="dialog"
      aria-label={labels.title}
      data-testid="control-panel"
      style={{ left: shown.x, top: shown.y, width: shown.width, height: shown.height }}
    >
      <header className="control-panel-title" data-testid="control-panel-title" onPointerDown={begin("move")} onPointerMove={follow} onPointerUp={finish} onPointerCancel={finish}>
        <h2>{labels.title}</h2>
        <button type="button" className="dialog-close" aria-label={labels.close} data-testid="control-panel-close" onClick={onClose}>
          ×
        </button>
      </header>
      <div className="control-panel-grid">
        {PANEL_BUTTONS.map((button) => (
          <button
            key={button}
            type="button"
            className="control-panel-button"
            title={labels[button]}
            aria-label={TEXT[button] === null ? undefined : labels[button]}
            disabled={!isEnabled(actions[button])}
            data-testid={`control-${button}`}
            onClick={() => actions[button]?.run?.()}
          >
            {TEXT[button] ?? (button === "main" ? labels.mainShort : labels.paintShort)}
          </button>
        ))}
      </div>
      <span className="control-panel-size" aria-hidden="true" data-testid="control-panel-size" onPointerDown={begin("size")} onPointerMove={follow} onPointerUp={finish} onPointerCancel={finish} />
    </section>
  );
}
