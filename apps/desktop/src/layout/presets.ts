import type { CustomLayout, LayoutPanels, WindowLayout } from "@/api/backend";
import { type Display, PANEL_FIELDS } from "../display/settings";

/** 显示 → 布局模式's entries, by their menu ids. */
export type Preset = "defaultView" | "classicView" | "minView" | "extraMode3" | "extraMode1" | "extraMode2";

const ALL_PANELS_ON = Object.fromEntries(PANEL_FIELDS.map((field) => [field, true])) as Record<(typeof PANEL_FIELDS)[number], true>;

/**
 * What each entry changes (`LizzieFrame.defaultMode` / `classicMode` / `minMode` / `extraMode`)
 * and Java's window width as a share of its height, null where Java computes it otherwise.
 */
export const PRESETS: Record<Preset, { patch: Partial<Display>; ratio: number | null }> = {
  defaultView: { patch: { layout_mode: "Normal", ...ALL_PANELS_ON, large_sub_board: false, large_winrate_graph: false }, ratio: 1.65 },
  classicView: {
    patch: {
      layout_mode: "Normal",
      show_status_bar: false,
      show_variation_graph: false,
      show_comment: false,
      show_sub_board: true,
      show_winrate_graph: true,
      show_information_panel: true,
      large_sub_board: true,
      large_winrate_graph: false,
    },
    ratio: 1.45,
  },
  // Java's minMode switches the panels off; the status line it only leaves undrawn.
  minView: {
    patch: { layout_mode: "Min", show_sub_board: false, show_comment: false, show_information_panel: false, show_list_pane: false, show_variation_graph: false, show_winrate_graph: false },
    ratio: null,
  },
  extraMode3: {
    patch: { layout_mode: "Thinking", show_winrate_graph: true, show_comment: false, show_information_panel: true, show_list_pane: true, show_variation_graph: true, large_winrate_graph: false },
    ratio: 1.66,
  },
  extraMode1: {
    patch: { layout_mode: "MultiView", show_winrate_graph: true, show_comment: false, show_information_panel: true, show_list_pane: true, show_variation_graph: true, large_winrate_graph: false },
    ratio: 1.62,
  },
  // Two boards side by side and nothing else: Java widens the window to twice the boards' height.
  extraMode2: { patch: { layout_mode: "DoubleEngine" }, ratio: 1.75 },
};

/** The window as the page measures it: its content, the main panel, the bottom toolbar. */
export interface Measures {
  innerWidth: number;
  innerHeight: number;
  panelWidth: number;
  panelHeight: number;
  bottomToolbar: number;
}

/**
 * The window content's new size for an entry: Java's width as a share of the height above the
 * bottom toolbar, the height kept; 精简模式 a near square around the board.
 */
export function presetWindow(preset: Preset, measures: Measures): { width: number; height: number } {
  const { ratio } = PRESETS[preset];
  if (ratio !== null) return { width: Math.round((measures.innerHeight - measures.bottomToolbar) * ratio), height: measures.innerHeight };
  const chrome = measures.innerHeight - measures.panelHeight;
  const side = Math.min(measures.panelWidth, measures.panelHeight);
  return { width: Math.round(side * 0.94), height: side + chrome };
}

/** The panels' switches as a layout keeps them. */
export function panelsOf(display: Display): Required<LayoutPanels> {
  return Object.fromEntries(PANEL_FIELDS.map((field) => [field, display[field]])) as Required<LayoutPanels>;
}

/** 自定义布局 → 设置 → 应用: what the slot keeps of the window as it is now. */
export function customLayoutOf(display: Display): CustomLayout {
  const layout: CustomLayout = {
    panels: panelsOf(display),
    layout_mode: display.layout_mode,
    large_sub_board: display.large_sub_board,
    large_winrate_graph: display.large_winrate_graph,
    board_position: display.board_position,
    floating_boards: display.floating_boards,
    floating_main: display.floating_main,
    floating_sub: display.floating_sub,
  };
  // A window never moved has no saved place; the slot then keeps none and leaves it where it is.
  if (display.main_window) layout.main_window = display.main_window;
  return layout;
}

/** The display half of choosing a slot: its panels, mode, enlargements and board position. */
export function displayOf(layout: CustomLayout): Partial<Display> {
  return {
    ...layout.panels,
    ...(layout.layout_mode ? { layout_mode: layout.layout_mode } : {}),
    large_sub_board: layout.large_sub_board ?? false,
    large_winrate_graph: layout.large_winrate_graph ?? false,
    ...(layout.board_position === undefined ? {} : { board_position: layout.board_position }),
  };
}

/** The window half of choosing a slot, for `apply_window_layout`. */
export function windowsOf(layout: CustomLayout): WindowLayout {
  return {
    main_window: layout.main_window ?? null,
    ...(layout.floating_boards ? { floating_boards: layout.floating_boards } : {}),
    floating_main: layout.floating_main ?? null,
    floating_sub: layout.floating_sub ?? null,
  };
}
