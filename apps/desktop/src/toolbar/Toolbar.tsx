import type { EngineStatus } from "@/api/backend";
import { Icon } from "./icons";

/** Home / End step size: the backend stops at the first or last move. */
export const JUMP = 1_000_000;

export interface ToolbarLabels {
  open: string;
  save: string;
  first: string;
  previous: string;
  next: string;
  last: string;
  pause: string;
  resume: string;
  move: string;
}

export interface ToolbarProps {
  labels: ToolbarLabels;
  moveNumber: number | null;
  pondering: boolean;
  canSave: boolean;
  onOpen(): void;
  onSave(): void;
  onStep(steps: number): void;
  onTogglePondering(): void;
}

export function Toolbar({ labels, moveNumber, pondering, canSave, onOpen, onSave, onStep, onTogglePondering }: ToolbarProps) {
  return (
    <nav className="toolbar" aria-label={labels.move}>
      <span className="tool-group">
        <ToolButton icon="open" label={labels.open} testId="open-file" onClick={onOpen} />
        <ToolButton icon="save" label={labels.save} testId="save-file" onClick={onSave} disabled={!canSave} />
      </span>
      <span className="tool-group">
        <ToolButton icon="first" label={labels.first} testId="nav-first" onClick={() => onStep(-JUMP)} compact />
        <ToolButton icon="previous" label={labels.previous} testId="nav-previous" onClick={() => onStep(-1)} compact />
        <span className="move-number" title={labels.move}>
          <strong data-testid="move-number">{moveNumber ?? "—"}</strong>
        </span>
        <ToolButton icon="next" label={labels.next} testId="nav-next" onClick={() => onStep(1)} compact />
        <ToolButton icon="last" label={labels.last} testId="nav-last" onClick={() => onStep(JUMP)} compact />
      </span>
      <ToolButton
        icon={pondering ? "pause" : "resume"}
        label={pondering ? labels.pause : labels.resume}
        testId="toggle-pondering"
        pressed={!pondering}
        onClick={onTogglePondering}
      />
    </nav>
  );
}

function ToolButton(props: {
  icon: Parameters<typeof Icon>[0]["name"];
  label: string;
  testId: string;
  onClick(): void;
  disabled?: boolean;
  compact?: boolean;
  pressed?: boolean;
}) {
  return (
    <button
      type="button"
      className={props.compact ? "tool-button compact" : "tool-button"}
      data-testid={props.testId}
      title={props.label}
      aria-label={props.compact ? props.label : undefined}
      aria-pressed={props.pressed}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      <Icon name={props.icon} />
      {props.compact ? null : <span>{props.label}</span>}
    </button>
  );
}

export function EngineLight({ status, labels }: { status: EngineStatus; labels: Record<EngineStatus["type"], string> }) {
  const detail =
    status.type === "Failed" || status.type === "Restarting"
      ? status.data.reason
      : status.type === "Ready"
        ? status.data.version
        : undefined;
  return (
    <span className="status engine" data-testid="engine-status" data-state={status.type} title={detail}>
      <span className="engine-dot" data-state={status.type} aria-hidden="true" />
      {labels[status.type]}
    </span>
  );
}
