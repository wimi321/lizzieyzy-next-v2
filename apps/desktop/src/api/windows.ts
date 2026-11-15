import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import type { FloatingBoardKind } from "./generated/FloatingBoardKind";
import type { FloatingBoardsView } from "./generated/FloatingBoardsView";
import type { DisplaySettings } from "./generated/DisplaySettings";
import type { WindowLayout } from "./generated/WindowLayout";

export type { FloatingBoardKind, FloatingBoardsView, WindowLayout };
export type { WindowBounds } from "./generated/WindowBounds";
export type { WindowPlacement } from "./generated/WindowPlacement";
export type { CustomLayout } from "./generated/CustomLayout";
export type { LayoutMode } from "./generated/LayoutMode";
export type { LayoutPanels } from "./generated/LayoutPanels";
export type ResizeEdge = "North" | "South" | "East" | "West" | "NorthEast" | "NorthWest" | "SouthEast" | "SouthWest";
export type FloatingBoardsChangedListener = (view: FloatingBoardsView) => void;
export type DisplaySettingsChangedListener = (display: DisplaySettings) => void;

export interface WindowCommands {
  fitMainWindow(width: number, height: number): Promise<void>;
  resetWindowPositions(): Promise<void>;
  openFloatingBoard(kind: FloatingBoardKind): Promise<FloatingBoardsView>;
  closeFloatingBoard(kind: FloatingBoardKind): Promise<FloatingBoardsView>;
  setFloatingBoardOptions(kind: FloatingBoardKind, onTop: boolean, locked: boolean): Promise<FloatingBoardsView>;
  setFloatingLayout(main: boolean, keepMainBoard: boolean, sub: boolean): Promise<FloatingBoardsView>;
  applyWindowLayout(layout: WindowLayout): Promise<DisplaySettings>;
  /** A floating board moves with the pointer: the system takes the drag from here. */
  dragWindow(): Promise<void>;
  /** A floating board is sized from an edge or corner. */
  resizeWindow(edge: ResizeEdge): Promise<void>;
  /** A floating board stays square, as Java's do. */
  setWindowSquare(side: number): Promise<void>;
}

/** Sizes and placements are logical pixels; validation and fitting belong to Rust. */
export const fitMainWindow: NonNullable<WindowCommands["fitMainWindow"]> = (width, height) =>
  invoke("fit_main_window", { width, height });
export const resetWindowPositions: NonNullable<WindowCommands["resetWindowPositions"]> = () =>
  invoke("reset_window_positions");
export const openFloatingBoard: NonNullable<WindowCommands["openFloatingBoard"]> = (kind) =>
  invoke("open_floating_board", { kind });
export const closeFloatingBoard: NonNullable<WindowCommands["closeFloatingBoard"]> = (kind) =>
  invoke("close_floating_board", { kind });
export const setFloatingBoardOptions: NonNullable<WindowCommands["setFloatingBoardOptions"]> = (kind, onTop, locked) =>
  invoke("set_floating_board_options", { kind, onTop, locked });
export const setFloatingLayout: NonNullable<WindowCommands["setFloatingLayout"]> = (main, keepMainBoard, sub) =>
  invoke("set_floating_layout", { main, keepMainBoard, sub });
/** Apply a saved layout's native window state through the serialized shell path. */
export const applyWindowLayout: NonNullable<WindowCommands["applyWindowLayout"]> = (layout) =>
  invoke("apply_window_layout", { layout });
export const dragWindow: WindowCommands["dragWindow"] = () => getCurrentWindow().startDragging();
export const resizeWindow: WindowCommands["resizeWindow"] = (edge) => getCurrentWindow().startResizeDragging(edge);
export const setWindowSquare: WindowCommands["setWindowSquare"] = (side) => getCurrentWindow().setSize(new LogicalSize(side, side));
