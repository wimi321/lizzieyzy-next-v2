import { type ReactNode, useCallback, useEffect, useState } from "react";
import type { Backend, ColorView, DocumentView, MarkupKindView, PointView } from "@/api/backend";
import { ConfirmDialog } from "../frame/ConfirmDialog";
import { EditModeChip } from "./EditModeChip";

export type SetupTool = "Black" | "White" | "Erase";

/** What a click on the board does: play, edit the starting position, or mark the current move. */
export type BoardMode = { kind: "play" } | { kind: "setup"; tool: SetupTool } | { kind: "markup"; tool: MarkupKindView | null };

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
  done: string;
  doneHint: string;
  confirm: string;
  cancel: string;
}

type ModeBackend = Pick<Backend, "setSetupStone" | "clearSetup" | "setFirstPlayer" | "convertToStartPosition" | "setMarkup" | "clearMarkup" | "toggleToPlay">;

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

/** Where a click goes in each mode; null means the board plays as usual. */
export function clickCommand(mode: BoardMode, point: PointView): { setup: [PointView, ColorView | null] } | { markup: [PointView, MarkupKindView] } | null {
  if (mode.kind === "setup") return { setup: [point, mode.tool === "Erase" ? null : mode.tool] };
  if (mode.kind === "markup" && mode.tool) return { markup: [point, mode.tool] };
  return null;
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
  const editing = (mode.kind === "setup") || (mode.kind === "markup" && mode.tool !== null);

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
    else backend.setMarkup(...command.markup).catch(report);
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
    exit,
    chip,
    dialog,
    dialogOpen: convert !== null,
  };
}
