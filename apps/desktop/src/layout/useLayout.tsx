import { useState } from "react";
import type { Backend, CustomLayout } from "@/api/backend";
import { arrangementOf, type Display, filled } from "../display/settings";
import type { MenuBindings } from "../menu/bindings";
import type en from "../locales/en.json";
import { PanelSettingsDialog } from "./PanelSettingsDialog";
import { customLayoutOf, displayOf, type Measures, type Preset, PRESETS, presetWindow, windowsOf } from "./presets";
import { SideBoards, type SideBoardsProps } from "./SideBoards";

export type LayoutBackend = Pick<Backend, "fitMainWindow" | "resetWindowPositions" | "applyWindowLayout" | "settings" | "previewVariation">;

function report(error: unknown) {
  console.error("Layout command failed", error);
}

/** The window's content, main panel and bottom toolbar as laid out now. */
function measure(): Measures {
  const panel = document.querySelector<HTMLElement>('[data-testid="main-panel"]');
  const bottom = document.querySelector<HTMLElement>('[data-testid="bottom-toolbar"]');
  return {
    innerWidth: window.innerWidth,
    innerHeight: window.innerHeight,
    panelWidth: panel?.clientWidth ?? window.innerWidth,
    panelHeight: panel?.clientHeight ?? window.innerHeight,
    bottomToolbar: bottom?.offsetHeight ?? 0,
  };
}

/** The panel-settings dialog: null for 可视化面板设置, else the slot; and the display it opened on. */
type Editing = { slot: number | null; before: Display };

/**
 * 布局模式, 放大小棋盘 / 放大胜率图, 主棋盘/界面位置, 恢复默认面板尺寸, 重置界面位置,
 * 可视化面板设置 and 自定义布局 1 / 2: their entries, the layout they give and their dialog.
 */
export function useLayout(backend: LayoutBackend, display: Display, set: (patch: Partial<Display>) => void, labels: typeof en.layout) {
  const [editing, setEditing] = useState<Editing | null>(null);

  // Java resizes the window to suit the layout; Rust leaves a maximized window alone.
  const choose = (preset: Preset) => {
    const size = presetWindow(preset, measure());
    set(PRESETS[preset].patch);
    void backend.fitMainWindow(size.width, size.height).catch(report);
  };
  const apply = (layout: CustomLayout) => {
    set(displayOf(layout));
    void backend.applyWindowLayout(windowsOf(layout)).catch(report);
  };
  const slot = (index: number) => () => {
    const saved = display.custom_layouts[index];
    if (saved) apply(saved);
    else setEditing({ slot: index, before: display });
  };
  const edit = (index: number | null) => {
    setEditing({ slot: index, before: display });
    const saved = index === null ? null : display.custom_layouts[index];
    if (saved) set(displayOf(saved));
  };
  const finish = () => {
    if (editing?.slot != null) {
      const index = editing.slot;
      // The windows' places are Rust's: the shell saves them as they move.
      void backend
        .settings()
        .then((settings) => {
          const { main_window, floating_main, floating_sub, floating_boards } = filled(settings.display);
          const layouts = [...display.custom_layouts] as Display["custom_layouts"];
          layouts[index] = customLayoutOf({ ...display, main_window, floating_main, floating_sub, floating_boards });
          set({ custom_layouts: layouts });
        })
        .catch(report);
    }
    setEditing(null);
  };
  const cancel = () => {
    if (editing) set(dialogFields(editing.before));
    setEditing(null);
  };

  const mode = display.layout_mode;
  const largeSub = display.large_sub_board && display.show_sub_board;
  const largeGraph = display.large_winrate_graph && display.show_winrate_graph;
  const position = display.board_position;
  const bindings: MenuBindings = {
    defaultView: { run: () => choose("defaultView") },
    classicView: { run: () => choose("classicView") },
    minView: { run: () => choose("minView") },
    // A checked mode chosen again goes back to the default layout (Java only applied it again).
    extraMode3: { run: () => choose(mode === "Thinking" ? "defaultView" : "extraMode3"), checked: mode === "Thinking" },
    extraMode1: { run: () => choose(mode === "MultiView" ? "defaultView" : "extraMode1"), checked: mode === "MultiView" },
    largeSubBoard: { run: () => set({ large_sub_board: !largeSub }), checked: largeSub },
    largeWinrateGraph: { run: () => set({ large_winrate_graph: !largeGraph }), checked: largeGraph },
    leftMove: { run: () => set({ board_position: position - 1 }), enabled: position > 0 },
    rightMove: { run: () => set({ board_position: position + 1 }), enabled: position < 8 },
    restoreDefaultPanelSizes: { run: () => set({ board_position: 4 }) },
    deletePersistFile: { run: () => void backend.resetWindowPositions().catch(report) },
    visualizedPanelSettings: { run: () => edit(null) },
    customeMode1: { run: slot(0), aside: { label: labels.setUp, run: () => edit(0) } },
    customeMode2: { run: slot(1), aside: { label: labels.setUp, run: () => edit(1) } },
  };

  return {
    bindings,
    arrangement: arrangementOf(display),
    dialogOpen: editing !== null,
    /** 思考模式's plain board or 四方图's four boards, for the main panel's extra square. */
    extra: (props: Omit<SideBoardsProps, "mode" | "backend" | "labels">) =>
      mode === "Thinking" || mode === "MultiView" ? <SideBoards mode={mode} backend={backend} labels={labels.boards} {...props} /> : undefined,
    dialog: editing ? (
      <PanelSettingsDialog slot={editing.slot} display={display} labels={labels.dialog} onChange={set} onApply={finish} onCancel={cancel} />
    ) : null,
  };
}

/** What the dialog (and a slot shown in it) can change: put back as it was on 取消. */
function dialogFields(display: Display): Partial<Display> {
  const { large_sub_board, large_winrate_graph, board_position, layout_mode } = display;
  return { ...customLayoutOf(display).panels, large_sub_board, large_winrate_graph, board_position, layout_mode };
}
