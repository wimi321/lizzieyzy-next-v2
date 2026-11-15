import { useState } from "react";
import { ConfirmDialog } from "../frame/ConfirmDialog";
import type en from "../locales/en.json";
import type { Slot } from "./useSaveSlots";

type Labels = typeof en.research.saves;

/** The slot's position as a small board, drawn from the stones Rust sends with it. */
export function SlotThumb({ board }: { board: NonNullable<Slot["board"]> }) {
  const { width, height } = board;
  const lines: string[] = [];
  for (let x = 0; x < width; x++) lines.push(`M${x} 0V${height - 1}`);
  for (let y = 0; y < height; y++) lines.push(`M0 ${y}H${width - 1}`);
  return (
    <svg className="slot-thumb" viewBox={`-0.7 -0.7 ${width + 0.4} ${height + 0.4}`} role="img" aria-hidden="true">
      <rect x={-0.7} y={-0.7} width={width + 0.4} height={height + 0.4} fill="#dcb35c" />
      <path d={lines.join("")} stroke="#3a2b12" strokeWidth={0.06} fill="none" />
      {board.stones.map((stone, index) =>
        stone ? (
          <circle
            key={index}
            cx={index % width}
            cy={Math.floor(index / width)}
            r={0.47}
            fill={stone === "Black" ? "#111" : "#f6f6f2"}
            stroke={stone === "Black" ? "#000" : "#777"}
            strokeWidth={0.05}
          />
        ) : null,
      )}
    </svg>
  );
}

type Ask = { kind: "overwrite" | "delete"; slot: Slot } | { kind: "deleteAll" };

export interface SavePanelProps {
  /** null until the list has been read. */
  slots: readonly Slot[] | null;
  autosave: boolean;
  restore: boolean;
  error: string | null;
  locale: string;
  labels: Labels;
  onLoad(id: string): void;
  onOverwrite(id: string): void;
  onRename(id: string, name: string): void;
  onDelete(id: string): void;
  onDeleteAll(): void;
  onNew(name: string): void;
  onAutosave(enabled: boolean): void;
  onRestore(enabled: boolean): void;
  onClose(): void;
}

function SlotCard({ slot, number, locale, labels, onLoad, onRename, onAsk }: { slot: Slot; number: number | null; locale: string; labels: Labels; onLoad(): void; onRename(name: string): void; onAsk(kind: "overwrite" | "delete"): void }) {
  const [name, setName] = useState(slot.name);
  const usable = !slot.corrupt;
  const editable = usable && !slot.is_autosave;
  const time = slot.saved_at_unix_ms > 0 ? new Intl.DateTimeFormat(locale, { dateStyle: "short", timeStyle: "short" }).format(slot.saved_at_unix_ms) : "";
  return (
    <article className="save-slot" data-testid="save-slot" data-slot-id={slot.id} data-corrupt={slot.corrupt ? "" : undefined} data-autosave={slot.is_autosave ? "" : undefined}>
      <div className="save-slot-board">{usable && slot.board ? <SlotThumb board={slot.board} /> : <span className="save-slot-corrupt">{slot.corrupt ? labels.corrupt : ""}</span>}</div>
      <div className="save-slot-name">
        <span className="save-slot-number">{number === null ? labels.autosaveSlot : number}</span>
        {editable ? (
          <input className="text-input" value={name} aria-label={labels.name} maxLength={120} data-testid="save-slot-name" onChange={(event) => setName(event.target.value)} />
        ) : (
          <span className="save-slot-label">{slot.name}</span>
        )}
      </div>
      <div className="save-slot-meta">
        <span>{time}</span>
        {usable ? <span>{labels.moves.replace("{n}", String(slot.move_number))}</span> : null}
      </div>
      <div className="save-slot-actions">
        {usable ? (
          <button type="button" className="secondary-button" data-testid="save-slot-load" onClick={onLoad}>
            {labels.load}
          </button>
        ) : null}
        {editable ? (
          <>
            <button type="button" className="secondary-button" data-testid="save-slot-overwrite" onClick={() => onAsk("overwrite")}>
              {labels.overwrite}
            </button>
            <button type="button" className="secondary-button" disabled={name.trim() === "" || name.trim() === slot.name} data-testid="save-slot-rename" onClick={() => onRename(name.trim())}>
              {labels.rename}
            </button>
          </>
        ) : null}
        {!slot.is_autosave ? (
          <button type="button" className="secondary-button" data-testid="save-slot-delete" onClick={() => onAsk("delete")}>
            {labels.delete}
          </button>
        ) : null}
      </div>
    </article>
  );
}

/**
 * 存档与读档: Java's panel over the list and the tree. One card per slot, the autosave first and
 * read-only, then 新建存档; damaged slots can only be deleted.
 */
export function SavePanel({ slots, autosave, restore, error, locale, labels, ...on }: SavePanelProps) {
  const [ask, setAsk] = useState<Ask | null>(null);
  const [name, setName] = useState("");
  const kept = slots?.filter((slot) => !slot.is_autosave) ?? [];
  const automatic = slots?.find((slot) => slot.is_autosave);
  const confirm = () => {
    if (!ask) return;
    setAsk(null);
    if (ask.kind === "deleteAll") on.onDeleteAll();
    else if (ask.kind === "delete") on.onDelete(ask.slot.id);
    else on.onOverwrite(ask.slot.id);
  };
  const card = (slot: Slot, number: number | null) => (
    <SlotCard
      key={`${slot.id}:${slot.name}`}
      slot={slot}
      number={number}
      locale={locale}
      labels={labels}
      onLoad={() => on.onLoad(slot.id)}
      onRename={(next) => on.onRename(slot.id, next)}
      onAsk={(kind) => setAsk({ kind, slot })}
    />
  );
  return (
    <section className="save-panel" aria-label={labels.title} data-testid="save-panel">
      <header className="save-panel-bar">
        <label className="jt-check">
          <input type="checkbox" checked={autosave} data-testid="save-autosave" onChange={(event) => on.onAutosave(event.target.checked)} />
          <span>{labels.autosave}</span>
        </label>
        <label className="jt-check">
          <input type="checkbox" checked={restore} data-testid="save-restore" onChange={(event) => on.onRestore(event.target.checked)} />
          <span>{labels.restore}</span>
        </label>
        <span className="save-panel-gap" />
        <button type="button" className="secondary-button" disabled={kept.length === 0} data-testid="save-delete-all" onClick={() => setAsk({ kind: "deleteAll" })}>
          {labels.deleteAll}
        </button>
        <button type="button" className="secondary-button" data-testid="save-close" onClick={on.onClose}>
          {labels.close}
        </button>
      </header>
      {error ? (
        <p className="save-panel-error" role="alert" data-testid="save-error">
          {error}
        </p>
      ) : null}
      <div className="save-panel-slots">
        {automatic ? card(automatic, null) : null}
        {kept.map((slot, index) => card(slot, index + 1))}
        {slots ? (
          <article className="save-slot save-slot-new" data-testid="save-new">
            <div className="save-slot-board save-slot-plus" aria-hidden="true">
              +
            </div>
            <input className="text-input" value={name} placeholder={labels.namePlaceholder} aria-label={labels.name} maxLength={120} data-testid="save-new-name" onChange={(event) => setName(event.target.value)} />
            <button
              type="button"
              className="primary-button"
              data-testid="save-new-button"
              onClick={() => {
                on.onNew(name.trim());
                setName("");
              }}
            >
              {labels.newSlot}
            </button>
          </article>
        ) : (
          <p className="save-panel-loading">{labels.loading}</p>
        )}
      </div>
      {ask ? (
        <ConfirmDialog
          title={labels.title}
          lines={[ask.kind === "deleteAll" ? labels.deleteAllConfirm : ask.kind === "delete" ? labels.deleteConfirm.replace("{name}", ask.slot.name) : labels.overwriteConfirm]}
          confirm={labels.confirm}
          cancel={labels.cancel}
          testId="save-confirm"
          onConfirm={confirm}
          onCancel={() => setAsk(null)}
        />
      ) : null}
    </section>
  );
}
