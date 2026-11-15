import { useCallback, useState } from "react";
import type { Backend, SaveSlot, Settings, SlotBoard } from "@/api/backend";
import { reasonOf } from "../everyday/useEveryday";
import type en from "../locales/en.json";

type Labels = typeof en.research.saves;

export type SaveSlotsBackend = Pick<
  Backend,
  "saveSlots" | "saveToNewSlot" | "overwriteSlot" | "renameSlot" | "loadSlot" | "deleteSlot" | "deleteAllSlots" | "setAutosave" | "setRestoreLastGame"
>;

/** A slot with every field: Rust always sends them all; older payloads may leave some out. */
export type Slot = Required<Omit<SaveSlot, "board">> & { board: Required<SlotBoard> | null };

export function filledSlot(slot: SaveSlot): Slot {
  const board = slot.board ?? null;
  return {
    id: slot.id ?? "",
    name: slot.name ?? "",
    saved_at_unix_ms: slot.saved_at_unix_ms ?? 0,
    node_path: slot.node_path ?? [],
    move_number: slot.move_number ?? 0,
    board: board && board.width && board.height ? { width: board.width, height: board.height, stones: board.stones ?? [] } : null,
    is_autosave: slot.is_autosave ?? false,
    corrupt: slot.corrupt ?? false,
  };
}

/** A `SaveSlotsError`, bare or inside `CommandError::SaveSlots`, in the interface language. */
export function slotError(error: unknown, labels: Labels): string {
  let failure = typeof error === "object" && error !== null && "type" in error ? (error as { type: string; data?: unknown }) : null;
  if (failure?.type === "SaveSlots") failure = typeof failure.data === "object" && failure.data !== null && "type" in failure.data ? (failure.data as { type: string; data?: unknown }) : null;
  const text = failure ? (labels.errors as Record<string, string>)[failure.type] : undefined;
  if (!failure || text === undefined) return reasonOf(error);
  const data = (failure.data ?? {}) as { reason?: string };
  return text.replace("{reason}", data.reason ?? "");
}

/** 存档与读档's state: the slots as Rust lists them, read again after every change, and its two switches. */
export function useSaveSlots(backend: SaveSlotsBackend, labels: Labels) {
  const [open, setOpen] = useState(false);
  const [slots, setSlots] = useState<Slot[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [autosave, setAutosave] = useState(true);
  const [restore, setRestore] = useState(false);

  const load = useCallback((settings: Pick<Settings, "autosave" | "restore_last_game">) => {
    setAutosave(settings.autosave ?? true);
    setRestore(settings.restore_last_game ?? false);
  }, []);
  const fail = (failure: unknown) => setError(slotError(failure, labels));
  const refresh = () => {
    backend.saveSlots().then((listed) => setSlots(listed.map(filledSlot)), fail);
  };
  const change = (request: Promise<unknown>) => {
    setError(null);
    request.then(refresh, fail);
  };
  const show = () => {
    setOpen(true);
    setError(null);
    refresh();
  };

  return {
    open,
    show,
    toggle: () => (open ? setOpen(false) : show()),
    close: () => setOpen(false),
    load,
    panel: {
      slots,
      autosave,
      restore,
      error,
      onLoad: (id: string) => {
        setError(null);
        backend.loadSlot(id).catch(fail);
      },
      onOverwrite: (id: string) => change(backend.overwriteSlot(id)),
      onRename: (id: string, name: string) => change(backend.renameSlot(id, name)),
      onDelete: (id: string) => change(backend.deleteSlot(id)),
      onDeleteAll: () => change(backend.deleteAllSlots()),
      onNew: (name: string) => change(backend.saveToNewSlot(name)),
      onAutosave: (enabled: boolean) => {
        backend.setAutosave(enabled).then(setAutosave, fail);
      },
      onRestore: (enabled: boolean) => {
        backend.setRestoreLastGame(enabled).then(setRestore, fail);
      },
      onClose: () => setOpen(false),
    },
  };
}
