import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import zh from "../locales/zh-CN.json";
import { BottomToolbar, type BottomToolbarProps } from "./BottomToolbar";
import { type FitItem, type FitRow, fitToolbar } from "./toolbarFit";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const item = (id: string | null, width: number): FitItem => ({ id, width });
const OPTIONS = { padding: 10, gap: 8, priority: ["remote", "kata", "coords", "delete", "weights", "autoPlay"], more: "more", moreAlways: true };
// 暂停分析 and 自动分析 always stay, the navigation too; the rest gives way by priority.
const ROW: FitRow = {
  left: [item(null, 90), item(null, 75), item("remote", 75), item("weights", 100), item("kata", 75)],
  center: [item(null, 376), item("autoPlay", 75)],
  right: [item("coords", 50), item("delete", 50), item("more", 50)],
};

describe("the bottom toolbar at a given width", () => {
  test("a wide window keeps every button, the navigation centred", () => {
    expect(fitToolbar(ROW, { ...OPTIONS, width: 1400 })).toEqual({ hidden: [], start: 470.5, centered: true, leftTrack: 452.5 });
  });

  test("narrower: 远程算力 stays; a wide button that does not fit leaves its room to a narrower one", () => {
    const fit = fitToolbar(ROW, { ...OPTIONS, width: 1000 });
    expect(fit.hidden).toEqual(["weights"]);
    // The navigation moves right of centre so the left buttons do not run into it.
    expect(fit.centered).toBe(false);
    expect(fit.start).toBe(10 + 90 + 8 + 75 + 8 + 75 + 8 + 75 + 8);
    expect(fit.leftTrack).toBe(fit.start - 18);
  });

  test("the lowest priorities go first, 远程算力 last", () => {
    expect(fitToolbar(ROW, { ...OPTIONS, width: 800 }).hidden).toEqual(["weights", "kata", "autoPlay", "delete"]);
    expect(fitToolbar(ROW, { ...OPTIONS, width: 700 }).hidden).toEqual(["remote", "weights", "kata", "autoPlay", "delete"]);
  });

  test("更多 comes only when something has no room, and its own width is kept for it", () => {
    const options = { ...OPTIONS, moreAlways: false };
    expect(fitToolbar(ROW, { ...options, width: 1400 }).hidden).toEqual(["more"]);
    // Without 更多 only 自动播放 would give way; with it, 更新官方权重 does instead.
    expect(fitToolbar(ROW, { ...options, width: 1000 }).hidden).toEqual(["weights"]);
  });

  test("when even the fixed buttons do not fit, the left side keeps its place", () => {
    const fit = fitToolbar(ROW, { ...OPTIONS, width: 500 });
    expect(fit.hidden).toEqual(["remote", "weights", "kata", "autoPlay", "coords", "delete"]);
    expect(fit.start).toBe(10 + 90 + 8 + 75 + 8);
  });
});

function props(overrides: Partial<BottomToolbarProps> = {}): BottomToolbarProps {
  const noop = vi.fn();
  return {
    labels: zh.frame.bottom,
    pondering: false,
    hasBoard: true,
    ownership: false,
    coordinates: true,
    canSetMain: true,
    canDelete: true,
    trial: false,
    autoAnalysis: null,
    onTogglePondering: noop,
    onToggleOwnership: noop,
    onToggleCoordinates: noop,
    onToggleMoveNumbers: noop,
    onStep: noop,
    onSetMain: noop,
    onClear: noop,
    onDelete: noop,
    onTrial: noop,
    onRemote: noop,
    autoPlay: { playing: false, onClick: noop },
    ...overrides,
  };
}

/** jsdom lays nothing out: the bar is `width` wide and each button as wide as its text. */
function narrowWindow(width: number) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const size = this.dataset.testid === "bottom-toolbar" ? width : (this.textContent?.length ?? 0) * 13 + 24;
    return { x: 0, y: 0, left: 0, top: 0, right: size, bottom: 25, width: size, height: 25, toJSON: () => ({}) } as DOMRect;
  });
}

describe("远程算力 in a narrow window", () => {
  test("stays on the bar while lower buttons move into 更多, which runs them", () => {
    narrowWindow(850);
    const onClear = vi.fn();
    render(<BottomToolbar {...props({ onClear })} />);
    const bar = screen.getByTestId("bottom-toolbar");
    expect(screen.getByTestId("tool-remote").className).not.toContain("jt-fit-hidden");
    expect(screen.getByTestId("toggle-ownership").className).not.toContain("jt-fit-hidden");
    expect(screen.getByTestId("bottom-clear").className).toContain("jt-fit-hidden");
    expect(bar.dataset.fitHidden).toBe("downloadWeight autoPlay setMainButton clearButton");

    fireEvent.click(screen.getByTestId("tool-more"));
    const menu = screen.getByTestId("tool-more-menu");
    expect(within(menu).getAllByRole("menuitem").map((entry) => entry.textContent)).toEqual(["更新官方权重", "自动播放", "设为主分支", "清空棋盘", "试下", "终局点目", "纯网络"]);
    fireEvent.click(within(menu).getByTestId("more-bottom-clear"));
    expect(onClear).toHaveBeenCalledOnce();
    expect(screen.queryByTestId("tool-more-menu")).toBeNull();
  });

  test("a wide window shows every button and 更多 keeps only its own entries", () => {
    narrowWindow(2000);
    render(<BottomToolbar {...props()} />);
    expect(screen.getByTestId("bottom-toolbar").dataset.fitHidden).toBeUndefined();
    fireEvent.click(screen.getByTestId("tool-more"));
    expect(within(screen.getByTestId("tool-more-menu")).getAllByRole("menuitem").map((entry) => entry.textContent)).toEqual(["试下", "终局点目", "纯网络"]);
  });

  test("without 试下, 终局点目 and 纯网络, 更多 shows only when something has no room", () => {
    const show = (id: string) => !["tryPlay", "finalScore", "heatMap", "openfile", "savefile", "badMoves", "analyzeList", "refresh", "backMain", "moveRank"].includes(id);
    narrowWindow(2000);
    const { unmount } = render(<BottomToolbar {...props({ show })} />);
    expect(screen.getByTestId("tool-more").parentElement?.className).toBe("jt-fit-hidden");
    unmount();
    narrowWindow(900);
    render(<BottomToolbar {...props({ show })} />);
    expect(screen.getByTestId("tool-more").parentElement?.className).toBe("");
    fireEvent.click(screen.getByTestId("tool-more"));
    expect(within(screen.getByTestId("tool-more-menu")).queryByRole("separator")).toBeNull();
  });
});
