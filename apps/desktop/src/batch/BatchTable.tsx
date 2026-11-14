import { useEffect, useState } from "react";
import type { BatchAnalysisStatus, BatchItem, BatchMove } from "@/api/backend";
import type { BatchLabels } from "./labels";

export interface BatchTableProps {
  status: BatchAnalysisStatus;
  labels: BatchLabels;
  onAdd(): void;
  onRemove(id: number): void;
  onMove(id: number, direction: BatchMove): void;
  onClear(): void;
  /** Starts the waiting games, or stops the running one. */
  onStartStop(): void;
  onPauseResume(): void;
  /** Opens a finished game's analysed copy in the window. */
  onOpen(path: string): void;
  onClose(): void;
}

const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

function itemText(item: BatchItem, labels: BatchLabels): string {
  const state = item.state ?? { type: "Waiting" };
  switch (state.type) {
    case "Waiting":
      return labels.waiting;
    case "Running": {
      const progress = state.data.progress;
      return progress && progress.total > 0
        ? labels.running.replace("{done}", String(progress.completed)).replace("{total}", String(progress.total))
        : labels.opening;
    }
    case "Completed":
      return labels.completed;
    case "Failed":
      return labels.failed.replace("{reason}", state.data.reason ?? "");
  }
}

/** Java's 批量分析进度表: the queue, what each game came to, and the controls. It stays open beside the board. */
export function BatchTable({ status, labels, onAdd, onRemove, onMove, onClear, onStartStop, onPauseResume, onOpen, onClose }: BatchTableProps) {
  const [selected, setSelected] = useState<number | null>(null);
  const items = status.items ?? [];
  const phase = status.phase ?? "Idle";
  const active = phase === "Running" || phase === "Paused";
  const chosen = items.find((item) => item.id === selected);
  const movable = chosen?.state?.type === "Waiting" && chosen.id !== undefined;
  const waiting = items.some((item) => item.state?.type === "Waiting");

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const phaseText = {
    Idle: labels.phaseIdle,
    Running: labels.phaseRunning,
    Paused: labels.phasePaused,
    Stopped: labels.phaseStopped,
    Completed: labels.phaseCompleted,
  }[phase];
  const move = (direction: BatchMove) => () => {
    if (movable && chosen?.id !== undefined) onMove(chosen.id, direction);
  };

  return (
    <section className="dialog batch-table" role="dialog" aria-modal="false" aria-labelledby="batch-table-title" data-testid="batch-table">
      <header className="dialog-header">
        <h2 id="batch-table-title">{labels.tableTitle}</h2>
        <span className="batch-phase" data-testid="batch-phase" data-phase={phase}>
          {phaseText}
          {status.reason ? ` · ${status.reason}` : ""}
        </span>
        <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
          ×
        </button>
      </header>
      <div className="batch-rows" role="table" aria-label={labels.tableTitle}>
        <div className="batch-row batch-head" role="row">
          <span role="columnheader">{labels.colIndex}</span>
          <span role="columnheader">{labels.colFile}</span>
          <span role="columnheader">{labels.colState}</span>
          <span role="columnheader">{labels.colOutput}</span>
        </div>
        {items.length === 0 ? <p className="settings-hint batch-empty">{labels.empty}</p> : null}
        {items.map((item, index) => {
          const state = item.state?.type ?? "Waiting";
          const output = item.state?.type === "Completed" ? item.state.data.output : undefined;
          const progress = item.state?.type === "Running" ? item.state.data.progress : null;
          const share = progress && progress.total > 0 ? progress.completed / progress.total : 0;
          return (
            <div
              key={item.id ?? index}
              className="batch-row"
              role="row"
              aria-selected={item.id === selected}
              data-testid="batch-row"
              data-state={state}
              onClick={() => setSelected(item.id ?? null)}
            >
              <span role="cell">{index + 1}</span>
              <span role="cell" className="batch-name" title={item.path}>
                {item.path ? baseName(item.path) : ""}
              </span>
              <span role="cell" className="batch-state" title={item.state?.type === "Failed" ? item.state.data.reason : undefined}>
                {state === "Running" ? <span className="batch-bar" style={{ width: `${share * 100}%` }} aria-hidden="true" /> : null}
                <span>{itemText(item, labels)}</span>
              </span>
              <span role="cell" className="batch-output">
                {output ? (
                  <button
                    type="button"
                    className="link-button"
                    data-testid="batch-open-output"
                    title={output}
                    disabled={active}
                    onClick={(event) => {
                      event.stopPropagation();
                      onOpen(output);
                    }}
                  >
                    {baseName(output)}
                  </button>
                ) : null}
              </span>
            </div>
          );
        })}
      </div>
      <footer className="batch-actions">
        <button type="button" className="secondary-button" data-testid="batch-add" onClick={onAdd}>
          {labels.add}
        </button>
        <button type="button" className="secondary-button" data-testid="batch-remove" disabled={!movable} onClick={() => chosen?.id !== undefined && onRemove(chosen.id)}>
          {labels.remove}
        </button>
        <button type="button" className="secondary-button" data-testid="batch-up" disabled={!movable} onClick={move("Up")}>
          {labels.up}
        </button>
        <button type="button" className="secondary-button" data-testid="batch-down" disabled={!movable} onClick={move("Down")}>
          {labels.down}
        </button>
        <button type="button" className="secondary-button" data-testid="batch-first" disabled={!movable} onClick={move("First")}>
          {labels.first}
        </button>
        <button type="button" className="secondary-button" data-testid="batch-clear" disabled={items.length === 0} onClick={onClear}>
          {labels.clear}
        </button>
        <span className="batch-spacer" />
        <button type="button" className="secondary-button" data-testid="batch-pause" disabled={!active} onClick={onPauseResume}>
          {phase === "Paused" ? labels.resume : labels.pause}
        </button>
        <button type="button" className="primary-button" data-testid="batch-start-stop" disabled={!active && !waiting} onClick={onStartStop}>
          {active ? labels.stop : labels.startQueue}
        </button>
      </footer>
    </section>
  );
}
