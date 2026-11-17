import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { CaptureCells, CaptureGrid, CaptureOutcome, CapturePermission, CaptureRecognition, DocumentView, TsumegoOptions } from "@/api/backend";
import { midGame } from "../../e2e/fixtures";
import zh from "../locales/zh-CN.json";
import { matchShortcut } from "../menu/shortcuts";
import { CapturePage, type CapturePageBackend } from "./CapturePage";
import { BOARD_SIZE_KEY, cellAt, cycled, defaultBoardSize, fits, gridLines, makesCells, pngSize, previewLines, storedBoardSize, toImage } from "./geometry";
import { type CaptureBackend, startError, useCapture } from "./useCapture";
import { describe as describeError, LatestOnly } from "./values";

const labels = zh.capture;

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

/** A PNG's signature and IHDR size, which is all the page reads from the bytes. */
function png(width: number, height: number): ArrayBuffer {
  const bytes = new Uint8Array(32);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], 0);
  const view = new DataView(bytes.buffer);
  view.setUint32(16, width);
  view.setUint32(20, height);
  return bytes.buffer;
}

const GRID: CaptureGrid = { origin: { x: 49.5, y: 49.5 }, gap_x: 36, gap_y: 36, columns: 6, rows: 6, corner: "TopLeft" };
const CELLS: CaptureCells = {
  rows: 6,
  columns: 6,
  cells: ["......", "......", "....W.", "...B.B", "......", "..B..."].flatMap((row) => [...row].map((c) => (c === "B" ? "Black" : c === "W" ? "White" : "Empty"))),
};
const RECOGNITION: CaptureRecognition = { grid: GRID, cells: CELLS };

describe("the capture window's geometry", () => {
  test("a PNG's size from its header; other bytes are not a PNG", () => {
    expect(pngSize(png(1920, 1080))).toEqual({ width: 1920, height: 1080 });
    expect(pngSize(new Uint8Array(32).buffer)).toBeNull();
  });

  test("pointer positions in the screenshot's own pixels", () => {
    expect(toImage(150, 60, { left: 100, top: 10, width: 960, height: 540 }, { width: 1920, height: 1080 })).toEqual({ x: 100, y: 100 });
  });

  test("Java's preview: five lines each way from the corner, towards the pointer", () => {
    const lines = previewLines({ x: 100, y: 100 }, { x: 80, y: 130 });
    expect(lines).toHaveLength(10);
    expect(lines).toContainEqual({ x1: 20, y1: 100, x2: 20, y2: 220 });
    expect(lines).toContainEqual({ x1: 100, y1: 220, x2: 20, y2: 220 });
  });

  test("the fitted grid's lines, the intersection a click lands on, and corrections", () => {
    expect(gridLines(GRID)).toHaveLength(12);
    expect(gridLines(GRID)[0]).toEqual({ x1: 49.5, y1: 49.5, x2: 49.5, y2: 229.5 });
    expect(cellAt(GRID, { x: 85, y: 156 })).toEqual({ row: 3, column: 1 });
    expect(cellAt(GRID, { x: 20, y: 20 })).toBeNull();
    expect(cellAt(GRID, { x: 260, y: 60 })).toBeNull();
    const once = cycled(CELLS, 3, 3);
    expect(once.cells[21]).toBe("White");
    expect(cycled(cycled(once, 3, 3), 3, 3).cells[21]).toBe("Black");
    expect(CELLS.cells[21]).toBe("Black");
  });

  test("board sizes: the current game's when the problem fits there, else the smallest that does", () => {
    expect(fits(CELLS, 9)).toBe(true);
    expect(defaultBoardSize(CELLS, 19)).toBe(19);
    expect(defaultBoardSize(CELLS, 13)).toBe(13);
    expect(defaultBoardSize(CELLS, 7)).toBe(9);
    expect(defaultBoardSize({ rows: 11, columns: 4, cells: [] }, 9)).toBe(13);
    expect(defaultBoardSize({ rows: 20, columns: 4, cells: [] }, 19)).toBeNull();
    expect(makesCells({ x: 1, y: 1 }, { x: 1, y: 5 })).toBe(false);
    expect(storedBoardSize({ getItem: () => "13" })).toBe(13);
    expect(storedBoardSize({ getItem: () => null })).toBeNull();
    expect(storedBoardSize({ getItem: () => { throw new Error("refused"); } })).toBeNull();
  });
});

describe("the live preview", () => {
  test("only the newest pointer, one request at a time, at most every 50 ms; dropped replies are ignored", async () => {
    vi.useFakeTimers();
    let clock = 0;
    const replies: Array<(value: string) => void> = [];
    const send = vi.fn((value: number) => new Promise<string>((resolve) => replies.push((reply) => resolve(`${reply}:${value}`))));
    const results: Array<string | null> = [];
    const latest = new LatestOnly(send, (result) => results.push(result), 50, () => clock);
    latest.push(1);
    latest.push(2);
    latest.push(3);
    expect(send.mock.calls.map(([value]) => value)).toEqual([1]);
    clock = 10;
    await act(async () => replies[0]!("a"));
    // The reply came quickly: the newest waits for the interval.
    expect(send.mock.calls.map(([value]) => value)).toEqual([1]);
    clock = 50;
    act(() => vi.advanceTimersByTime(40));
    expect(send.mock.calls.map(([value]) => value)).toEqual([1, 3]);
    latest.drop();
    await act(async () => replies[1]!("b"));
    expect(results).toEqual(["a:1"]);
  });
});

function pageBackend() {
  return {
    captureImage: vi.fn(() => Promise.resolve(png(748, 748))),
    recognizeCapture: vi.fn(() => Promise.resolve(RECOGNITION)),
    finishCapture: vi.fn(() => Promise.resolve(5)),
    cancelCapture: vi.fn(() => Promise.resolve()),
  } satisfies CapturePageBackend;
}

const flush = () => act(async () => undefined);

async function openPage(backend = pageBackend(), preferredSize: number | null = 19) {
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:shot");
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
  const view = render(<CapturePage backend={backend} session={7} labels={labels} preferredSize={preferredSize} />);
  await flush();
  vi.spyOn(screen.getByTestId("capture-area"), "getBoundingClientRect").mockReturnValue({ left: 0, top: 0, width: 748, height: 748, right: 748, bottom: 748, x: 0, y: 0, toJSON: () => ({}) } as DOMRect);
  return { backend, ...view };
}

const at = (x: number, y: number) => ({ clientX: x, clientY: y });

describe("the capture window", () => {
  test("three clicks, a live preview, then the result to correct and place", async () => {
    const { backend } = await openPage();
    const area = screen.getByTestId("capture-area");
    const page = screen.getByTestId("capture-page");
    expect(backend.captureImage).toHaveBeenCalledWith(7);
    expect(screen.getByTestId("capture-hint").textContent).toBe(labels.step1);

    fireEvent.click(area, at(49.5, 49.5));
    expect(page.dataset.stage).toBe("1");
    // A second click in the same column makes no cells.
    fireEvent.click(area, at(49.5, 85.5));
    expect(screen.getByTestId("capture-error").textContent).toBe(labels.page.gridError);
    fireEvent.click(area, at(85.5, 85.5));
    expect(page.dataset.stage).toBe("2");
    expect(screen.getByTestId("capture-hint").textContent).toBe(labels.step3);

    fireEvent.mouseMove(area, at(200, 200));
    await flush();
    expect(backend.recognizeCapture).toHaveBeenLastCalledWith(7, [{ x: 49.5, y: 49.5 }, { x: 85.5, y: 85.5 }, { x: 200, y: 200 }]);
    expect(screen.getAllByTestId("capture-dots")[0]!.querySelectorAll("circle")).toHaveLength(4);

    fireEvent.click(area, at(229.5, 229.5));
    await flush();
    expect(page.dataset.stage).toBe("confirm");
    expect(screen.getByTestId("capture-size").textContent).toBe("6 × 6 · 左上角");
    expect(screen.getByTestId("capture-board-19").getAttribute("aria-checked")).toBe("true");

    // Correct D3 of the capture (row 3, column 3): black → white.
    fireEvent.click(area, at(49.5 + 3 * 36, 49.5 + 3 * 36));
    fireEvent.click(screen.getByTestId("capture-board-9"));
    fireEvent.click(screen.getByTestId("capture-finish"));
    await flush();
    const [, result] = backend.finishCapture.mock.calls[0]! as unknown as [number, { corner: string; board_size: number; cells: CaptureCells }];
    expect(result.corner).toBe("TopLeft");
    expect(result.board_size).toBe(9);
    expect(result.cells.cells[21]).toBe("White");
  });

  test("Backspace steps back, 重来 starts over, Esc cancels", async () => {
    const { backend } = await openPage();
    const area = screen.getByTestId("capture-area");
    const page = screen.getByTestId("capture-page");
    fireEvent.click(area, at(49.5, 49.5));
    fireEvent.click(area, at(85.5, 85.5));
    fireEvent.click(area, at(229.5, 229.5));
    await flush();
    fireEvent.keyDown(window, { key: "Backspace" });
    expect(page.dataset.stage).toBe("2");
    fireEvent.keyDown(window, { key: "Backspace" });
    expect(page.dataset.stage).toBe("1");
    fireEvent.click(area, at(85.5, 85.5));
    fireEvent.click(area, at(229.5, 229.5));
    await flush();
    fireEvent.click(screen.getByTestId("capture-again"));
    expect(page.dataset.stage).toBe("0");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(backend.cancelCapture).toHaveBeenCalledWith(7);
  });

  test("a problem larger than every board cannot be confirmed; a refused finish says why", async () => {
    const backend = pageBackend();
    backend.recognizeCapture.mockResolvedValue({ grid: { ...GRID, rows: 20 }, cells: { rows: 20, columns: 6, cells: Array<"Empty">(120).fill("Empty") } });
    await openPage(backend, 13);
    const area = screen.getByTestId("capture-area");
    fireEvent.click(area, at(49.5, 49.5));
    fireEvent.click(area, at(85.5, 85.5));
    fireEvent.click(area, at(229.5, 729.5));
    await flush();
    expect(screen.getByTestId("capture-too-large").textContent).toBe(labels.page.tooLarge);
    expect(screen.getByTestId<HTMLButtonElement>("capture-finish").disabled).toBe(true);

    cleanup();
    const refusing = pageBackend();
    refusing.finishCapture.mockRejectedValue({ type: "Capture", data: { type: "DoesNotFit" } });
    await openPage(refusing, 13);
    const again = screen.getByTestId("capture-area");
    fireEvent.click(again, at(49.5, 49.5));
    fireEvent.click(again, at(85.5, 85.5));
    fireEvent.click(again, at(229.5, 229.5));
    await flush();
    expect(screen.getByTestId("capture-board-13").getAttribute("aria-checked")).toBe("true");
    fireEvent.click(screen.getByTestId("capture-finish"));
    await flush();
    expect(screen.getByTestId("capture-error").textContent).toBe("没能生成棋局：DoesNotFit");
  });
});

function tsumegoHandle() {
  return {
    open: false,
    setOpen: vi.fn(),
    options: { to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1 } as TsumegoOptions,
    setOptions: vi.fn(),
    status: null,
    generate: vi.fn(() => Promise.resolve()),
    exit: vi.fn(),
  };
}

function mainBackend(permission: CapturePermission = "NotNeeded") {
  let ended: ((event: { outcome: CaptureOutcome }) => void) | null = null;
  const backend = {
    captureSettings: vi.fn(() => Promise.resolve({ black_offset: 120, black_percent: 25, white_offset: 120, white_percent: 15, gray_offset: 80 })),
    setCaptureSettings: vi.fn((settings: object) => Promise.resolve(settings)),
    capturePermission: vi.fn(() => Promise.resolve(permission)),
    requestCapturePermission: vi.fn(() => Promise.resolve(permission)),
    openCapturePermissionSettings: vi.fn(() => Promise.resolve()),
    startScreenCapture: vi.fn(() => Promise.resolve({ session: { id: 7, width: 748, height: 748, scale_factor: 1 }, logical_width: 748, logical_height: 748 })),
    subscribeCaptureEnded: vi.fn((listener: (event: { outcome: CaptureOutcome }) => void) => {
      ended = listener;
      return Promise.resolve(4);
    }),
    unsubscribe: vi.fn(() => Promise.resolve()),
  } satisfies CaptureBackend;
  return { backend, end: (outcome: CaptureOutcome) => act(() => ended?.({ outcome })) };
}

async function mainWindow(backend: CaptureBackend, document: DocumentView | null = midGame(), tsumego = tsumegoHandle()) {
  const stored = new Map<string, string>();
  const storage = { setItem: (key: string, value: string) => void stored.set(key, value) };
  const host: { current: ReturnType<typeof useCapture> | null } = { current: null };
  function Harness() {
    const value = useCapture(backend, labels, { document, tsumego, tsumegoLabels: zh.tsumego, storage });
    host.current = value;
    return <div>{value.dialogs}</div>;
  }
  const view = render(<Harness />);
  await flush();
  return { ...view, host: () => host.current!, stored, tsumego };
}

describe("抓取死活题 in the main window", () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  test("the menu shows the steps; Shift+T and 开始抓取 start at once and leave the board size for the capture window", async () => {
    const { backend } = mainBackend();
    const { host, stored } = await mainWindow(backend);
    expect(matchShortcut({ code: "KeyT", key: "T", shiftKey: true, altKey: false, ctrlKey: false, metaKey: false, repeat: false })).toBe("captureTsumeGoNow");
    act(() => host().bindings.captureTsumeGo?.run?.());
    const dialog = screen.getByTestId("capture-dialog");
    expect(dialog.querySelectorAll("li")).toHaveLength(3);
    expect(dialog.textContent).toContain(labels.step3);
    fireEvent.click(screen.getByTestId("capture-start"));
    await flush();
    expect(backend.startScreenCapture).toHaveBeenCalledOnce();
    expect(screen.queryByTestId("capture-dialog")).toBeNull();
    expect(stored.get(BOARD_SIZE_KEY)).toBe("19");
    act(() => host().bindings.captureTsumeGoNow?.run?.());
    await flush();
    expect(backend.startScreenCapture).toHaveBeenCalledTimes(2);
  });

  test("unsaved changes are asked about first", async () => {
    const { backend } = mainBackend();
    const { host } = await mainWindow(backend, { ...midGame(), modified: true });
    act(() => host().bindings.captureTsumeGoNow?.run?.());
    expect(screen.getByTestId("capture-unsaved").textContent).toContain(labels.unsavedMessage);
    expect(backend.startScreenCapture).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText(labels.continue));
    await flush();
    expect(backend.startScreenCapture).toHaveBeenCalledOnce();
  });

  test("macOS without 屏幕录制: ask once, then explain; 打开系统设置 and 重试", async () => {
    const { backend } = mainBackend("Missing");
    const { host } = await mainWindow(backend);
    act(() => host().bindings.captureTsumeGoNow?.run?.());
    await flush();
    expect(backend.requestCapturePermission).toHaveBeenCalledOnce();
    expect(backend.startScreenCapture).not.toHaveBeenCalled();
    expect(screen.getByTestId("capture-permission").textContent).toContain("屏幕录制");
    fireEvent.click(screen.getByTestId("capture-open-settings"));
    expect(backend.openCapturePermissionSettings).toHaveBeenCalledOnce();
    backend.capturePermission.mockResolvedValue("Granted");
    fireEvent.click(screen.getByTestId("capture-retry"));
    await flush();
    expect(backend.requestCapturePermission).toHaveBeenCalledOnce();
    expect(backend.startScreenCapture).toHaveBeenCalledOnce();
  });

  test("a refused start says why", async () => {
    expect(startError({ type: "ScreenCapture", data: { type: "Busy" } }, labels)).toBe(labels.busy);
    expect(startError({ type: "ScreenCapture", data: { type: "Unsupported" } }, labels)).toBe(labels.unsupported);
    expect(startError({ type: "ScreenCapture", data: { type: "PermissionMissing" } }, labels)).toBe("permission");
    expect(startError({ type: "ScreenCapture", data: { type: "Failed", data: { reason: "no display" } } }, labels)).toBe("屏幕抓取没有成功：no display");
    expect(describeError(new Error("boom"))).toBe("boom");
    const { backend } = mainBackend();
    backend.startScreenCapture.mockRejectedValueOnce({ type: "ScreenCapture", data: { type: "Unsupported" } });
    const { host } = await mainWindow(backend);
    act(() => host().bindings.captureTsumeGoNow?.run?.());
    await flush();
    expect(screen.getByTestId("capture-toast").textContent).toBe(labels.unsupported);
  });

  test("the end: finished opens 死活题, cancelled shows the steps again, failed says why", async () => {
    const { backend, end } = mainBackend();
    const { tsumego } = await mainWindow(backend);
    end({ type: "Finished" });
    expect(tsumego.setOpen).toHaveBeenCalledWith(true);
    end({ type: "Cancelled" });
    expect(screen.getByTestId("capture-dialog")).toBeTruthy();
    end({ type: "Failed", data: { reason: "the window closed" } });
    expect(screen.getByTestId("capture-toast").textContent).toBe("屏幕抓取没有成功：the window closed");
  });

  test("参数设置: ranges are checked, defaults come back, saving returns to the steps", async () => {
    const { backend } = mainBackend();
    const { host } = await mainWindow(backend);
    act(() => host().bindings.captureTsumeGo?.run?.());
    fireEvent.click(screen.getByTestId("capture-settings"));
    await flush();
    fireEvent.change(screen.getByTestId("capture-field-black_percent"), { target: { value: "0" } });
    fireEvent.click(screen.getByTestId("capture-settings-save"));
    expect(screen.getByTestId("capture-settings-error").textContent).toBe("黑最低占比要是 1 到 100 之间的整数。");
    fireEvent.change(screen.getByTestId("capture-field-gray_offset"), { target: { value: "300" } });
    fireEvent.click(screen.getByTestId("capture-defaults"));
    fireEvent.change(screen.getByTestId("capture-field-white_offset"), { target: { value: "100" } });
    fireEvent.click(screen.getByTestId("capture-settings-save"));
    await flush();
    expect(backend.setCaptureSettings).toHaveBeenCalledWith({ black_offset: 120, white_offset: 100, black_percent: 25, white_percent: 15, gray_offset: 80 });
    expect(screen.getByTestId("capture-dialog")).toBeTruthy();
  });

  test("the 死活题 dialog's 屏幕抓取 closes it and shows the steps", async () => {
    const { backend } = mainBackend();
    const tsumego = { ...tsumegoHandle(), open: true };
    await mainWindow(backend, midGame(), tsumego);
    fireEvent.click(screen.getByTestId("tsumego-capture"));
    expect(tsumego.setOpen).toHaveBeenCalledWith(false);
    expect(screen.getByTestId("capture-dialog")).toBeTruthy();
  });
});

test("the hook unsubscribes when the window goes", async () => {
  const { backend } = mainBackend();
  const { unmount } = renderHook(() => useCapture(backend, labels, { document: null, tsumego: tsumegoHandle(), tsumegoLabels: zh.tsumego, storage: null }));
  await flush();
  unmount();
  expect(backend.unsubscribe).toHaveBeenCalledWith(4);
});
