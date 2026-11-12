import { type ReactNode, useCallback, useEffect, useState } from "react";
import type { Backend, ColorView, DocumentView, MarkupKindView, MoveView, PointView } from "@/api/backend";
import { ConfirmDialog } from "../frame/ConfirmDialog";
import { EditModeChip } from "./EditModeChip";

export type SetupTool = "Black" | "White" | "Erase";

/**
 * What a click on the board does: play, edit the starting position, mark the current move, play
 * one color whatever the turn (编辑 → 添加黑子), insert a move before the continuation, or put a
 * stone picked up with 移动 down elsewhere.
 */
export type BoardMode =
  | { kind: "play" }
  | { kind: "setup"; tool: SetupTool }
  | { kind: "markup"; tool: MarkupKindView | null }
  | { kind: "color"; color: ColorView }
  | { kind: "insert"; color: ColorView }
  | { kind: "move"; from: PointView };

export interface ModeLabels {
  setupTitle: string;
  setupBlack: string;
  setupWhite: string;
  setupErase: string;
  blackFirst: string;
  whiteFirst: string;
  clearSetup: string;
  convertTitle: string;
  convertMessage: string;
  markupTitle: string;
  markupLetter: string;
  markupNumber: string;
  markupCross: string;
  markupSquare: string;
  markupTriangle: string;
  markupCircle: string;
  markupEraser: string;
  clearMarkup: string;
  colorTitle: string;
  insertTitle: string;
  black: string;
  white: string;
  moveTitle: string;
  done: string;
  doneHint: string;
  confirm: string;
  cancel: string;
}

type ModeBackend = Pick<
  Backend,
  | "setSetupStone"
  | "clearSetup"
  | "setFirstPlayer"
  | "convertToStartPosition"
  | "setMarkup"
  | "clearMarkup"
  | "toggleToPlay"
  | "playColored"
  | "insertMove"
  | "moveStone"
>;

export interface BoardModes {
  mode: BoardMode;
  /** A tool is in hand: clicks go to `onPoint`, not to play. */
  editing: boolean;
  /** Setup stones can be edited: the game has no move yet, as Java requires. */
  setupEditable: boolean;
  onPoint(point: PointView): void;
  /** Takes up a setup tool; a game with moves is first made a starting position, after asking. */
  setupTool(tool: SetupTool): void;
  toggleSetup(): void;
  clearSetup(): void;
  setFirstPlayer(color: ColorView): void;
  convertToStart(): void;
  toggleMarkup(): void;
  toggleToPlay(): void;
  /** 添加黑子 / 添加白子: the same button again goes back to alternate play. */
  colorMode(color: ColorView): void;
  /** 插入黑子 / 插入白子 as a mode: every click inserts. */
  insertMode(color: ColorView): void;
  /** 移动: the stone at `from` goes where the next click lands. */
  startMove(from: PointView): void;
  /** A right click: in 添加 mode Java plays the other color; true when it was taken. */
  secondary(point: PointView): boolean;
  exit(): void;
  /** The chip for the strip's tray while a tool is in hand. */
  chip: ReactNode;
  dialog: ReactNode;
  dialogOpen: boolean;
}

const PLAY: BoardMode = { kind: "play" };

function report(error: unknown) {
  console.error("Board editing command failed", error);
}

export type ClickCommand =
  | { setup: [PointView, ColorView | null] }
  | { markup: [PointView, MarkupKindView] }
  | { colored: MoveView }
  | { insert: MoveView }
  | { move: [PointView, PointView] };

const placing = (color: ColorView, point: PointView): MoveView => ({ type: "Play", data: { color, point } });

/** Where a click goes in each mode; null means the board plays as usual. */
export function clickCommand(mode: BoardMode, point: PointView): ClickCommand | null {
  switch (mode.kind) {
    case "setup":
      return { setup: [point, mode.tool === "Erase" ? null : mode.tool] };
    case "markup":
      return mode.tool ? { markup: [point, mode.tool] } : null;
    case "color":
      return { colored: placing(mode.color, point) };
    case "insert":
      return { insert: placing(mode.color, point) };
    case "move":
      return { move: [mode.from, point] };
    default:
      return null;
  }
}

/**
 * The board's editing modes as LizzieYzy Next has them: 起始局面设置 with its three tools, and
 * the markup tools of the top toolbar, whose buttons toggle as Java's do.
 */
export function useBoardModes(backend: ModeBackend, document: DocumentView | null, labels: ModeLabels): BoardModes {
  const [mode, setMode] = useState<BoardMode>(PLAY);
  // A pending conversion: the tool to take up once the game is a starting position.
  const [convert, setConvert] = useState<{ then: SetupTool | null } | null>(null);
  const setupEditable = document !== null && document.tree.main_line.length === 1;

  const exit = useCallback(() => setMode(PLAY), []);
  const editing = mode.kind !== "play" && !(mode.kind === "markup" && mode.tool === null);

  // Esc puts the tool down, unless a field or dialog has the key.
  useEffect(() => {
    if (mode.kind === "play") return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest("input, textarea, select, [role='dialog'], [role='alertdialog'], [role='menu']")) return;
      setMode(PLAY);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [mode.kind]);

  // Another game without the board: nothing to edit.
  if (document === null && mode.kind !== "play") setMode(PLAY);

  const setupTool = (tool: SetupTool) => {
    if (setupEditable) setMode({ kind: "setup", tool });
    else setConvert({ then: tool });
  };

  const onPoint = (point: PointView) => {
    const command = clickCommand(mode, point);
    if (!command) return;
    if ("setup" in command) backend.setSetupStone(...command.setup).catch(report);
    else if ("markup" in command) backend.setMarkup(...command.markup).catch(report);
    else if ("colored" in command) backend.playColored(command.colored).catch(report);
    else if ("insert" in command) backend.insertMove(command.insert).catch(report);
    else {
      // The stone is put down once; the board goes back to play.
      setMode(PLAY);
      if (command.move[0].x !== point.x || command.move[0].y !== point.y) backend.moveStone(...command.move).catch(report);
    }
  };

  const setupTools = [
    { value: "Black" as const, label: labels.setupBlack, glyph: "●" },
    { value: "White" as const, label: labels.setupWhite, glyph: "○" },
    { value: "Erase" as const, label: labels.setupErase, glyph: "⌫" },
  ];
  const markupTools = [
    { value: "Letter" as const, label: labels.markupLetter, glyph: "A" },
    { value: "Number" as const, label: labels.markupNumber, glyph: "1" },
    { value: "Cross" as const, label: labels.markupCross, glyph: "✕" },
    { value: "Square" as const, label: labels.markupSquare, glyph: "□" },
    { value: "Triangle" as const, label: labels.markupTriangle, glyph: "△" },
    { value: "Circle" as const, label: labels.markupCircle, glyph: "◯" },
    { value: "Erase" as const, label: labels.markupEraser, glyph: "⌫" },
  ];

  let chip: ReactNode = null;
  if (mode.kind === "setup") {
    chip = (
      <EditModeChip
        title={labels.setupTitle}
        tools={setupTools}
        tool={mode.tool}
        actions={[
          { label: labels.blackFirst, testId: "setup-black-first", enabled: setupEditable, onClick: () => backend.setFirstPlayer("Black").catch(report) },
          { label: labels.whiteFirst, testId: "setup-white-first", enabled: setupEditable, onClick: () => backend.setFirstPlayer("White").catch(report) },
          { label: labels.clearSetup, testId: "setup-clear", enabled: setupEditable, onClick: () => backend.clearSetup().catch(report) },
        ]}
        done={labels.done}
        doneHint={labels.doneHint}
        testId="setup-chip"
        onTool={(tool) => setMode({ kind: "setup", tool })}
        onDone={exit}
      />
    );
  } else if (mode.kind === "markup") {
    chip = (
      <EditModeChip
        title={labels.markupTitle}
        tools={markupTools}
        tool={mode.tool}
        actions={[{ label: labels.clearMarkup, testId: "markup-clear", onClick: () => backend.clearMarkup().catch(report) }]}
        done={labels.done}
        doneHint={labels.doneHint}
        testId="markup-chip"
        // As in Java, the tool in hand is put down by its own button.
        onTool={(tool) => setMode({ kind: "markup", tool: mode.tool === tool ? null : tool })}
        onDone={exit}
      />
    );
  }

  const colors = [
    { value: "Black" as const, label: labels.black, glyph: "●" },
    { value: "White" as const, label: labels.white, glyph: "○" },
  ];
  if (mode.kind === "color" || mode.kind === "insert") {
    const kind = mode.kind;
    chip = (
      <EditModeChip
        title={kind === "color" ? labels.colorTitle : labels.insertTitle}
        tools={colors}
        tool={mode.color}
        done={labels.done}
        doneHint={labels.doneHint}
        testId={`${kind}-chip`}
        onTool={(color) => setMode({ kind, color })}
        onDone={exit}
      />
    );
  } else if (mode.kind === "move") {
    chip = (
      <EditModeChip title={labels.moveTitle} tools={[]} tool={null} done={labels.cancel} doneHint={labels.doneHint} testId="move-chip" onTool={() => {}} onDone={exit} />
    );
  }

  const dialog = convert ? (
    <ConfirmDialog
      title={labels.convertTitle}
      lines={[labels.convertMessage]}
      confirm={labels.confirm}
      cancel={labels.cancel}
      testId="confirm-convert"
      onConfirm={() => {
        const then = convert.then;
        setConvert(null);
        backend
          .convertToStartPosition()
          .then(() => {
            if (then) setMode({ kind: "setup", tool: then });
          })
          .catch(report);
      }}
      onCancel={() => setConvert(null)}
    />
  ) : null;

  return {
    mode,
    editing,
    setupEditable,
    onPoint,
    setupTool,
    toggleSetup: () => (mode.kind === "setup" ? exit() : setupTool("Black")),
    clearSetup: () => void backend.clearSetup().catch(report),
    setFirstPlayer: (color) => void backend.setFirstPlayer(color).catch(report),
    convertToStart: () => setConvert({ then: null }),
    toggleMarkup: () => setMode(mode.kind === "markup" ? PLAY : { kind: "markup", tool: null }),
    toggleToPlay: () => void backend.toggleToPlay().catch(report),
    colorMode: (color) => setMode(mode.kind === "color" && mode.color === color ? PLAY : { kind: "color", color }),
    insertMode: (color) => setMode(mode.kind === "insert" && mode.color === color ? PLAY : { kind: "insert", color }),
    startMove: (from) => setMode({ kind: "move", from }),
    secondary: (point) => {
      if (mode.kind !== "color") return false;
      backend.playColored(placing(mode.color === "Black" ? "White" : "Black", point)).catch(report);
      return true;
    },
    exit,
    chip,
    dialog,
    dialogOpen: convert !== null,
  };
}
