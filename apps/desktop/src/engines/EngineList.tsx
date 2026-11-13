import { useEffect } from "react";
import type { Engine } from "./model";

export interface EngineListLabels {
  title: string;
  intro: string;
  main: string;
  setMain: string;
  edit: string;
  remove: string;
  up: string;
  down: string;
  add: string;
  empty: string;
  close: string;
}

export interface EngineListProps {
  engines: readonly Engine[];
  main: string | null;
  /** The name shown for each engine, as in the menu. */
  nameOf(engine: Engine): string;
  /** A command is running; the buttons wait for it. */
  busy: boolean;
  labels: EngineListLabels;
  onSetMain(id: string): void;
  onEdit(id: string): void;
  onRemove(id: string): void;
  onMove(id: string, by: -1 | 1): void;
  onAdd(): void;
  onClose(): void;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** 设置 → 引擎: the configured engines in Java's order, the main one marked. */
export function EngineList({ engines, main, nameOf, busy, labels, onSetMain, onEdit, onRemove, onMove, onAdd, onClose }: EngineListProps) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog engine-list-dialog" role="dialog" aria-modal="true" aria-labelledby="engine-list-title" data-testid="engine-list">
        <header className="dialog-header">
          <h2 id="engine-list-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <p className="dialog-intro">{labels.intro}</p>
        {engines.length === 0 ? <p className="engine-list-empty">{labels.empty}</p> : null}
        <ol className="engine-list">
          {engines.map((engine, index) => {
            const isMain = engine.id === main;
            return (
              <li key={engine.id} className="engine-row" data-testid="engine-row" data-main={isMain || undefined}>
                <span className="engine-index">{`[${index + 1}]`}</span>
                <span className="engine-name">
                  <strong>{nameOf(engine)}</strong>
                  <span className="engine-files" title={`${engine.profile.program}\n${engine.profile.model}`}>
                    {`${fileName(engine.profile.program)} · ${fileName(engine.profile.model)}`}
                  </span>
                </span>
                {isMain ? <span className="engine-main-tag">{labels.main}</span> : null}
                <span className="engine-actions">
                  {isMain ? null : (
                    <button type="button" className="secondary-button" disabled={busy} data-testid="engine-set-main" onClick={() => onSetMain(engine.id)}>
                      {labels.setMain}
                    </button>
                  )}
                  <button type="button" className="secondary-button" disabled={busy} data-testid="engine-edit" onClick={() => onEdit(engine.id)}>
                    {labels.edit}
                  </button>
                  <button type="button" className="icon-button" disabled={busy || index === 0} aria-label={labels.up} title={labels.up} onClick={() => onMove(engine.id, -1)}>
                    ↑
                  </button>
                  <button
                    type="button"
                    className="icon-button"
                    disabled={busy || index === engines.length - 1}
                    aria-label={labels.down}
                    title={labels.down}
                    onClick={() => onMove(engine.id, 1)}
                  >
                    ↓
                  </button>
                  <button type="button" className="danger-link" disabled={busy} data-testid="engine-remove" onClick={() => onRemove(engine.id)}>
                    {labels.remove}
                  </button>
                </span>
              </li>
            );
          })}
        </ol>
        <footer className="dialog-footer">
          <button type="button" className="primary-button" disabled={busy} data-testid="engine-add" onClick={onAdd}>
            {labels.add}
          </button>
        </footer>
      </section>
    </div>
  );
}
