import { type ReactNode, useCallback, useEffect, useState } from "react";
import type { Backend, DocumentView, NewGameOptions } from "@/api/backend";
import { ConfirmDialog } from "../frame/ConfirmDialog";
import { BoardSizeDialog, type BoardSizeLabels } from "./BoardSizeDialog";
import { GameInfoDialog, type GameInfoLabels } from "./GameInfoDialog";
import { infoEdit, saveRoute } from "./info";

export interface EditLabels extends BoardSizeLabels, GameInfoLabels {
  noAgain: string;
  newBoardTitle: string;
  newBoardMessage: string;
  replaceTitle: string;
  replaceNotice: string;
  undo: string;
  redo: string;
  recentEmpty: string;
  clearRecent: string;
}

type EditBackend = Pick<
  Backend,
  | "newGame"
  | "setGameInfo"
  | "save"
  | "pickSavePath"
  | "openPath"
  | "recentFiles"
  | "clearRecentFiles"
  | "deleteNode"
  | "deleteBranch"
  | "promoteToMain"
  | "undo"
  | "redo"
>;

export type ConfirmKind = "newBoard" | "replace";

type Dialog =
  | { kind: "boardSize" }
  | { kind: "gameInfo" }
  | { kind: "confirmNew"; options: NewGameOptions }
  | { kind: "confirmReplace"; path: string };

export interface Editing {
  /** Recently opened or saved games, newest first. */
  recent: readonly string[];
  /** Whether the two questions are asked; Java's 显示 → 主界面设置 switches. */
  confirms: Readonly<Record<ConfirmKind, boolean>>;
  toggleConfirm(kind: ConfirmKind): void;
  /** An empty game of the current size, komi and rules. */
  newBoard(): void;
  openBoardSize(): void;
  openGameInfo(): void;
  /** Back to the game's own file; a game without one asks where, as 另存为 does. */
  save(): void;
  saveAs(): void;
  openRecent(path: string): void;
  clearRecent(): void;
  setKomi(komi: number): void;
  deleteMove(): void;
  deleteBranch(): void;
  setAsMain(): void;
  undo(): void;
  redo(): void;
  /** A dialog of this hook is open, so the board's shortcuts rest. */
  dialogOpen: boolean;
  dialogs: ReactNode;
}

function report(error: unknown) {
  console.error("Editing command failed", error);
}

/**
 * P2-01's editing commands behind Java's entries: new board, board size, game info, save,
 * recent files, deleting and undoing. The menu, the toolbars and the shortcuts all call these.
 */
export function useEditing(
  backend: EditBackend,
  document: DocumentView | null,
  /** A fresh, empty game: nothing would be lost, so nothing is asked. */
  emptyGame: boolean,
  labels: EditLabels,
  ruleNames: Readonly<Record<string, string>>,
  /** Bumped by the app when the recent list may have changed without a document change. */
  recentSignal: unknown,
): Editing {
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [confirms, setConfirms] = useState<Record<ConfirmKind, boolean>>({ newBoard: true, replace: true });
  const [recent, setRecent] = useState<readonly string[]>([]);
  const [recentVersion, setRecentVersion] = useState(0);

  // Opening and saving both reorder the list; Rust keeps it, so read it back.
  const path = document?.file_path ?? null;
  useEffect(() => {
    let active = true;
    backend
      .recentFiles()
      .then((files) => {
        if (active) setRecent(files);
      })
      .catch(report);
    return () => {
      active = false;
    };
  }, [backend, path, recentSignal, recentVersion]);

  const current = document?.tree.current;
  const board = document?.board ?? null;

  const startGame = (options: NewGameOptions) => {
    setDialog(null);
    backend.newGame(options).catch(report);
  };
  const askNewGame = (width: number, height: number) => {
    const options: NewGameOptions = {
      width,
      height,
      komi: document?.game_info.komi ?? 7.5,
      handicap: 0,
      rules: document?.game_info.rules_preset ?? "chinese",
      black_name: null,
      black_rank: null,
      white_name: null,
      white_rank: null,
    };
    if (!emptyGame && confirms.newBoard) setDialog({ kind: "confirmNew", options });
    else startGame(options);
  };

  const write = (target: string) => {
    setDialog(null);
    backend.save(target).catch(report);
  };
  const saveAs = () => {
    backend
      .pickSavePath()
      .then((target) => (target ? backend.save(target) : undefined))
      .catch(report);
  };

  const close = useCallback(() => setDialog(null), []);
  const stopAsking = (kind: ConfirmKind, noAgain: boolean) => {
    if (noAgain) setConfirms((value) => ({ ...value, [kind]: false }));
  };

  let dialogs: ReactNode = null;
  if (dialog?.kind === "boardSize") {
    dialogs = <BoardSizeDialog width={board?.width ?? 19} height={board?.height ?? 19} labels={labels} onConfirm={askNewGame} onClose={close} />;
  } else if (dialog?.kind === "gameInfo" && document) {
    dialogs = (
      <GameInfoDialog info={document.game_info} labels={labels} ruleNames={ruleNames} onSave={(edit) => backend.setGameInfo(edit).then(close)} onClose={close} />
    );
  } else if (dialog?.kind === "confirmNew") {
    const { options } = dialog;
    dialogs = (
      <ConfirmDialog
        title={labels.newBoardTitle}
        lines={[labels.newBoardMessage]}
        noAgain={labels.noAgain}
        confirm={labels.confirm}
        cancel={labels.cancel}
        testId="confirm-new-board"
        onConfirm={(noAgain) => {
          stopAsking("newBoard", noAgain);
          startGame(options);
        }}
        onCancel={close}
      />
    );
  } else if (dialog?.kind === "confirmReplace") {
    const target = dialog.path;
    dialogs = (
      <ConfirmDialog
        title={labels.replaceTitle}
        lines={[target, labels.replaceNotice]}
        noAgain={labels.noAgain}
        confirm={labels.confirm}
        cancel={labels.cancel}
        testId="confirm-replace"
        onConfirm={(noAgain) => {
          stopAsking("replace", noAgain);
          write(target);
        }}
        onCancel={close}
      />
    );
  }

  return {
    recent,
    confirms,
    toggleConfirm: (kind) => setConfirms((value) => ({ ...value, [kind]: !value[kind] })),
    newBoard: () => askNewGame(board?.width ?? 19, board?.height ?? 19),
    openBoardSize: () => setDialog({ kind: "boardSize" }),
    openGameInfo: () => {
      if (document) setDialog({ kind: "gameInfo" });
    },
    save: () => {
      const route = saveRoute(path, confirms.replace);
      if (route === "choose" || !path) saveAs();
      else if (route === "confirm") setDialog({ kind: "confirmReplace", path });
      else write(path);
    },
    saveAs,
    openRecent: (target) => {
      // A file that is gone is reported by Rust and leaves the list.
      backend
        .openPath(target)
        .catch(report)
        .finally(() => setRecentVersion((version) => version + 1));
    },
    clearRecent: () => {
      backend
        .clearRecentFiles()
        .then(() => setRecent([]))
        .catch(report);
    },
    setKomi: (komi) => {
      if (document && komi !== document.game_info.komi) backend.setGameInfo(infoEdit(document, { komi })).catch(report);
    },
    deleteMove: () => {
      if (current !== undefined) backend.deleteNode(current).catch(report);
    },
    deleteBranch: () => {
      if (current !== undefined) backend.deleteBranch(current).catch(report);
    },
    setAsMain: () => {
      if (current !== undefined) backend.promoteToMain(current).catch(report);
    },
    undo: () => void backend.undo().catch(report),
    redo: () => void backend.redo().catch(report),
    dialogOpen: dialog !== null,
    dialogs,
  };
}
