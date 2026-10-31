import type { ReactNode } from "react";
import type { EngineStatus } from "@/api/backend";
import { Icon, type IconName } from "./icons";

/** Home / End step size: the backend stops at the first or last move. */
export const JUMP = 1_000_000;

export interface TopBarLabels {
  open: string;
  save: string;
  fox: string;
  strength: string;
  first: string;
  previous: string;
  next: string;
  last: string;
  pause: string;
  resume: string;
  move: string;
  analysing: string;
  paused: string;
  coordinates: string;
  ownership: string;
  settings: string;
}

/** What the analysis chip shows; node and revision let tests match it to the board. */
export interface AnalysisBadge {
  nodeId: number;
  revision: number;
  visits: number;
  text: string;
  /** Search speed while the engine reports one, such as "1.2k/秒". */
  speed: string | null;
  title: string;
}

export interface TopBarProps {
  labels: TopBarLabels;
  engineLabels: Record<EngineStatus["type"], string>;
  moveNumber: number | null;
  /** Move number at the end of the displayed line. */
  lastMoveNumber: number | null;
  pondering: boolean;
  analysis: AnalysisBadge | null;
  noAnalysis: string;
  canSave: boolean;
  coordinates: boolean;
  ownership: boolean;
  engine: EngineStatus;
  /** Extra controls at the start of the right group, such as whole-game analysis. */
  extra?: ReactNode;
  onOpen(): void;
  onSave(): void;
  onOpenFox(): void;
  onOpenStrength(): void;
  onStep(steps: number): void;
  onTogglePondering(): void;
  onToggleCoordinates(): void;
  onToggleOwnership(): void;
  onOpenSettings(): void;
}

/** File actions left, move navigation centred, analysis and view controls right. */
export function TopBar(props: TopBarProps) {
  const { labels } = props;
  return (
    <header className="topbar">
      <div className="topbar-group">
        <IconButton icon="open" label={labels.open} testId="open-file" onClick={props.onOpen} showLabel />
        <IconButton icon="save" label={labels.save} testId="save-file" onClick={props.onSave} disabled={!props.canSave} showLabel />
        <span className="topbar-divider" aria-hidden="true" />
        <IconButton icon="fox" label={labels.fox} testId="open-fox" onClick={props.onOpenFox} showLabel />
        <IconButton icon="strength" label={labels.strength} testId="open-strength" onClick={props.onOpenStrength} showLabel />
      </div>
      <nav className="nav-pill" aria-label={labels.move}>
        <IconButton icon="first" label={labels.first} testId="nav-first" onClick={() => props.onStep(-JUMP)} />
        <IconButton icon="previous" label={labels.previous} testId="nav-previous" onClick={() => props.onStep(-1)} />
        <span className="move-counter" title={labels.move}>
          <strong data-testid="move-number">{props.moveNumber ?? "—"}</strong>
          {props.lastMoveNumber !== null ? <span>/ {props.lastMoveNumber}</span> : null}
        </span>
        <IconButton icon="next" label={labels.next} testId="nav-next" onClick={() => props.onStep(1)} />
        <IconButton icon="last" label={labels.last} testId="nav-last" onClick={() => props.onStep(JUMP)} />
      </nav>
      <div className="topbar-group end">
        {props.extra}
        <button
          type="button"
          className="chip"
          data-testid="toggle-pondering"
          data-state={props.pondering ? "live" : "paused"}
          aria-pressed={!props.pondering}
          title={props.pondering ? labels.pause : labels.resume}
          onClick={props.onTogglePondering}
        >
          <span className="live-dot" aria-hidden="true" />
          <span className="chip-label">{props.pondering ? labels.analysing : labels.paused}</span>
          <strong
            data-testid="analysis-move"
            data-node-id={props.analysis?.nodeId}
            data-revision={props.analysis?.revision}
            data-visits={props.analysis?.visits}
            title={props.analysis?.title}
          >
            {props.analysis?.text ?? props.noAnalysis}
          </strong>
          {props.analysis?.speed ? (
            <span className="chip-speed" data-testid="analysis-speed">
              {props.analysis.speed}
            </span>
          ) : null}
        </button>
        <span className="topbar-divider" aria-hidden="true" />
        <IconButton icon="coordinates" label={labels.coordinates} testId="toggle-coordinates" pressed={props.coordinates} onClick={props.onToggleCoordinates} />
        <IconButton icon="territory" label={labels.ownership} testId="toggle-ownership" pressed={props.ownership} onClick={props.onToggleOwnership} />
        <span className="topbar-divider" aria-hidden="true" />
        <EngineLight status={props.engine} labels={props.engineLabels} />
        <IconButton icon="settings" label={labels.settings} testId="open-setup" onClick={props.onOpenSettings} />
      </div>
    </header>
  );
}

function IconButton(props: {
  icon: IconName;
  label: string;
  testId: string;
  onClick(): void;
  disabled?: boolean;
  pressed?: boolean;
  showLabel?: boolean;
}) {
  return (
    <button
      type="button"
      className={props.showLabel ? "icon-button" : "icon-button square"}
      data-testid={props.testId}
      title={props.label}
      aria-label={props.showLabel ? undefined : props.label}
      aria-pressed={props.pressed}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      <Icon name={props.icon} />
      {props.showLabel ? <span>{props.label}</span> : null}
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
    <span className="engine" data-testid="engine-status" data-state={status.type} title={detail ?? labels[status.type]}>
      <span className="engine-dot" data-state={status.type} aria-hidden="true" />
      <span className="engine-label">{labels[status.type]}</span>
    </span>
  );
}
