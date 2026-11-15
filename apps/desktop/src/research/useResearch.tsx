import { useState } from "react";
import type { VariationTreeView } from "@/api/backend";
import type { Display } from "../display/settings";
import type { PanelCover } from "../frame/MainPanel";
import type { MenuBindings } from "../menu/bindings";
import type { MenuSubmenu } from "../menu/spec";
import type en from "../locales/en.json";
import { AutoPlayDialog } from "./AutoPlayDialog";
import { ControlPanel } from "./ControlPanel";
import { FunctionSearch } from "./FunctionSearch";
import { PaintOverlay } from "./PaintOverlay";
import { SavePanel } from "./SavePanel";
import { useAutoPlay } from "./useAutoPlay";
import { type SaveSlotsBackend, useSaveSlots } from "./useSaveSlots";

/** The top toolbar's 搜索功能, 独立棋局控制面板, 自由涂鸦 and 存档. */
export interface ResearchToolbar {
  onSearch(): void;
  controlPanel: boolean;
  onControlPanel(): void;
  saveLoad: boolean;
  onSaveLoad(): void;
  onPaint(): void;
}

/** The bottom toolbar's 自动播放: pressed while playing, and a click then stops it. */
export interface AutoPlayButton {
  playing: boolean;
  onClick(): void;
}

export interface ResearchContext {
  labels: typeof en.research;
  /** Menu labels by legacy key, in the interface language. */
  menuLabels: Readonly<Record<string, string>>;
  locale: string;
  display: Display;
  setDisplay(patch: Partial<Display>): void;
  hasBoard: boolean;
  /** The node shown, and the tree that says whether it is the last of its branch. */
  current: number | null;
  tree: VariationTreeView | null;
  step(steps: number): void;
}

/** 搜索功能, 自由涂鸦, 独立棋局控制面板, 存档与读档 and 自动播放: their state, entries and windows. */
export function useResearch(backend: SaveSlotsBackend, context: ResearchContext) {
  const { labels, display, setDisplay, hasBoard, step } = context;
  const [searchOpen, setSearchOpen] = useState(false);
  const [painting, setPainting] = useState(false);
  const [panelOpen, setPanelOpen] = useState(false);
  const [askAutoPlay, setAskAutoPlay] = useState(false);
  const saves = useSaveSlots(backend, labels.saves);
  const node = context.tree?.nodes.find((entry) => entry.id === context.current);
  const autoPlay = useAutoPlay(step, context.current, node !== undefined && node.children.length === 0);

  const paint = () => {
    if (hasBoard) setPainting(true);
  };
  const togglePanel = () => setPanelOpen(!panelOpen);
  const bindings: MenuBindings = {
    functionSearch: { run: () => setSearchOpen(true) },
    ctrlPanel: { run: togglePanel, checked: panelOpen },
    saveAndLoad: { run: saves.show },
    // While playing, the entry and Ctrl+A stop it: below 1300 px the bottom button gives way.
    makeAutoPlay: { run: () => (autoPlay.playing ? autoPlay.stop() : setAskAutoPlay(true)), enabled: hasBoard },
  };
  const startAutoPlay = (intervalMs: number) => {
    setAskAutoPlay(false);
    if (intervalMs !== display.autoplay_interval_ms) setDisplay({ autoplay_interval_ms: intervalMs });
    autoPlay.start(intervalMs);
  };

  return {
    bindings,
    load: saves.load,
    /** Windows that keep the keyboard while open. */
    dialogOpen: searchOpen || askAutoPlay,
    toolbar: {
      onSearch: () => setSearchOpen(true),
      controlPanel: panelOpen,
      onControlPanel: togglePanel,
      saveLoad: saves.open,
      onSaveLoad: saves.toggle,
      onPaint: paint,
    } satisfies ResearchToolbar,
    autoPlay: { playing: autoPlay.playing, onClick: () => (autoPlay.playing ? autoPlay.stop() : setAskAutoPlay(true)) } satisfies AutoPlayButton,
    paintLayer:
      painting && hasBoard ? (
        <PaintOverlay color={display.painting_color} labels={labels.paint} onColor={(painting_color) => setDisplay({ painting_color })} onClose={() => setPainting(false)} />
      ) : null,
    cover: saves.open ? ({ regions: ["list", "tree"], node: <SavePanel {...saves.panel} locale={context.locale} labels={labels.saves} /> } satisfies PanelCover) : undefined,
    /** Rendered with the menus and every binding, which 搜索功能 lists and the control panel's buttons use. */
    windows: (menus: readonly MenuSubmenu[], all: MenuBindings) => (
      <>
        {panelOpen ? (
          <ControlPanel
            bounds={display.control_panel_bounds}
            labels={labels.panel}
            actions={{
              first: all.jumpToFirst,
              last: all.jumpToLast,
              back5: { run: () => step(-5), enabled: hasBoard },
              forward5: { run: () => step(5), enabled: hasBoard },
              back: all.previousMove,
              forward: all.nextMove,
              main: all.backToMainBranch,
              paint: { run: paint, enabled: hasBoard },
            }}
            onBounds={(control_panel_bounds) => setDisplay({ control_panel_bounds })}
            onClose={() => setPanelOpen(false)}
          />
        ) : null}
        {searchOpen ? <FunctionSearch menus={menus} bindings={all} menuLabels={context.menuLabels} labels={labels.search} onClose={() => setSearchOpen(false)} /> : null}
        {askAutoPlay ? <AutoPlayDialog intervalMs={display.autoplay_interval_ms} labels={labels.autoPlay} onStart={startAutoPlay} onClose={() => setAskAutoPlay(false)} /> : null}
      </>
    ),
  };
}
