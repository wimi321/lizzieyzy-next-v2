import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useEffect } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { BoardTransform, BoardView, Settings } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { isEnabled, type MenuBinding } from "../menu/bindings";
import { matchShortcut } from "../menu/shortcuts";
import { localeOf } from "../release/useRelease";
import { useEveryday, type EverydayView } from "./useEveryday";

const capture = vi.hoisted(() => ({
  canvasPng: vi.fn((testId: string) => Promise.resolve(new Uint8Array([137, 80, 78, 71, testId.length]))),
  svgPng: vi.fn(() => Promise.resolve(new Uint8Array([137, 80, 78, 71, 0]))),
}));
const sound = vi.hoisted(() => ({ playStone: vi.fn() }));
vi.mock("./capture", () => capture);
vi.mock("./sound", () => sound);

afterEach(cleanup);
beforeEach(() => {
  vi.clearAllMocks();
  document.documentElement.style.zoom = "";
});

function board(width: number, height: number, move_number = 3, last: "Play" | "Pass" | null = "Play"): BoardView {
  const last_move =
    last === "Play"
      ? ({ type: "Play", data: { color: "Black", point: { x: 3, y: 3 } } } as const)
      : last === "Pass"
        ? ({ type: "Pass", data: { color: "Black" } } as const)
        : null;
  return { width, height, move_number, last_move } as unknown as BoardView;
}

function everydayBackend() {
  return {
    copySgf: vi.fn(() => Promise.resolve()),
    pasteSgf: vi.fn(() => Promise.resolve(4)),
    saveCleanSgf: vi.fn(() => Promise.resolve()),
    savePng: vi.fn<(path: string, bytes: Uint8Array) => Promise<void>>(() => Promise.resolve()),
    copyPng: vi.fn<(bytes: Uint8Array) => Promise<void>>(() => Promise.resolve()),
    transformBoard: vi.fn<(kind: BoardTransform) => Promise<number>>(() => Promise.resolve(5)),
    setAlwaysOnTop: vi.fn((enabled: boolean) => Promise.resolve(enabled)),
    setUiScale: vi.fn((scale: number) => Promise.resolve(scale)),
    setPlaySound: vi.fn((enabled: boolean) => Promise.resolve(enabled)),
    pickSavePath: vi.fn<(name?: string) => Promise<string | null>>(() => Promise.resolve("/games/对局-raw.sgf")),
    pickImagePath: vi.fn<(name: string) => Promise<string | null>>(() => Promise.resolve("/pictures/对局-3.png")),
  };
}

type Backend = ReturnType<typeof everydayBackend>;
let bindings: Record<string, MenuBinding> = {};
let load: (settings: Settings) => void = () => {};
function expose(everyday: ReturnType<typeof useEveryday>) {
  bindings = everyday.bindings;
  load = everyday.load;
}

function Harness({ backend, view, readKomi }: { backend: Backend; view: EverydayView; readKomi?: { value: boolean; set(enabled: boolean): void } }) {
  const everyday = useEveryday(backend, view, readKomi ?? { value: true, set: () => {} });
  useEffect(() => expose(everyday));
  return everyday.dialogs(zh.everyday, zh.display);
}

const VIEW: EverydayView = { board: board(19, 19), fileName: "对局.sgf", subBoard: true, graph: true };

function mount(backend = everydayBackend(), view: EverydayView = VIEW, readKomi?: { value: boolean; set(enabled: boolean): void }) {
  const result = render(<Harness backend={backend} view={view} readKomi={readKomi} />);
  return { backend, rerender: (next: EverydayView) => result.rerender(<Harness backend={backend} view={next} readKomi={readKomi} />) };
}

const run = (id: string) => act(() => bindings[id]?.run?.());

test("更多保存 saves clean records under a name taken from the game; a cancelled dialog saves nothing", async () => {
  const { backend } = mount();
  await run("saveRaw");
  await run("saveCommentRaw");
  await run("saveBranchRaw");
  await vi.waitFor(() => expect(backend.saveCleanSgf).toHaveBeenCalledTimes(3));
  expect(backend.pickSavePath.mock.calls.map(([name]) => name)).toEqual(["对局-raw.sgf", "对局-raw.sgf", "对局-branch.sgf"]);
  expect(backend.saveCleanSgf.mock.calls).toEqual([
    ["/games/对局-raw.sgf", false, false],
    ["/games/对局-raw.sgf", true, false],
    ["/games/对局-raw.sgf", false, true],
  ]);
  backend.pickSavePath.mockResolvedValueOnce(null);
  await run("saveRaw");
  await vi.waitFor(() => expect(backend.pickSavePath).toHaveBeenCalledTimes(4));
  expect(backend.saveCleanSgf).toHaveBeenCalledTimes(3);
});

test("pictures are taken before the save dialog opens, saved or copied; hidden panels can't be taken", async () => {
  const { backend, rerender } = mount();
  await run("saveMainBoardScreen");
  await vi.waitFor(() => expect(backend.savePng).toHaveBeenCalledTimes(1));
  expect(capture.canvasPng.mock.invocationCallOrder[0]).toBeLessThan(backend.pickImagePath.mock.invocationCallOrder[0] ?? 0);
  expect(backend.pickImagePath).toHaveBeenCalledWith("对局-3.png");
  expect(backend.savePng.mock.calls[0]?.[1]).toEqual(new Uint8Array([137, 80, 78, 71, 5]));
  await run("saveWinrate");
  await vi.waitFor(() => expect(backend.savePng).toHaveBeenCalledTimes(2));
  expect(backend.pickImagePath).toHaveBeenLastCalledWith("对局-winrate.png");
  await run("copyBoardScreen");
  await run("copySubBoardScreen");
  await vi.waitFor(() => expect(backend.copyPng).toHaveBeenCalledTimes(2));
  expect(capture.canvasPng.mock.calls.map(([id]) => id)).toEqual(["board", "board", "sub-board"]);

  rerender({ ...VIEW, subBoard: false, graph: false });
  for (const id of ["saveSubBoardScreen", "copySubBoardScreen", "saveWinrate"]) expect(isEnabled(bindings[id])).toBe(false);
  rerender({ ...VIEW, board: null });
  for (const id of ["copySgf", "saveRaw", "saveMainBoardScreen", "mirrorVertical"]) expect(isEnabled(bindings[id])).toBe(false);
  expect(isEnabled(bindings.pasteSgf)).toBe(true);
});

test("copy and paste go through Rust; an empty clipboard and other failures are explained", async () => {
  const { backend } = mount();
  await run("copySgf");
  await run("pasteSgf");
  expect(backend.copySgf).toHaveBeenCalledTimes(1);
  expect(backend.pasteSgf).toHaveBeenCalledTimes(1);

  backend.pasteSgf.mockRejectedValueOnce({ type: "Clipboard", data: { type: "EmptyText" } });
  await run("pasteSgf");
  const box = await screen.findByTestId("everyday-failed");
  expect(box.textContent).toContain("剪贴板里没有文字。");
  fireEvent.click(screen.getByTestId("everyday-failed-ok"));

  backend.savePng.mockRejectedValueOnce({ type: "Png", data: { type: "Invalid", data: "not a PNG" } });
  await run("saveMainBoardScreen");
  const again = await screen.findByTestId("everyday-failed");
  expect(again.textContent).toContain("图片没能保存。");
  expect(again.textContent).toContain("not a PNG");
});

test("编辑 → 旋转 needs a square board; 水平翻转 and 垂直翻转 work on any board, named as Java names them", async () => {
  const { backend, rerender } = mount();
  await run("spinRight");
  await run("spinLeft");
  await run("mirrorVertical");
  await run("mirrorHorizon");
  expect(backend.transformBoard.mock.calls.map(([kind]) => kind)).toEqual(["RotateRight", "RotateLeft", "FlipHorizontal", "FlipVertical"]);
  rerender({ ...VIEW, board: board(13, 9) });
  expect(isEnabled(bindings.spinRight)).toBe(false);
  expect(isEnabled(bindings.spinLeft)).toBe(false);
  expect(isEnabled(bindings.mirrorVertical)).toBe(true);
  expect(isEnabled(bindings.mirrorHorizon)).toBe(true);
});

test("总在最前, 界面字号 and 播放落子声 follow the saved settings, save each change, and undo a refused one", async () => {
  const { backend } = mount();
  act(() => load({ always_on_top: true, ui_scale: 1.15, play_sound: false } as Settings));
  expect(bindings.alwaysOnTop?.checked).toBe(true);
  expect(bindings.frameFontSizeMiddle?.checked).toBe(true);
  expect(bindings.playSound?.checked).toBe(false);
  expect(document.documentElement.style.zoom).toBe("1.15");

  await run("frameFontSizeBig");
  expect(backend.setUiScale).toHaveBeenLastCalledWith(1.3);
  expect(bindings.frameFontSizeBig?.checked).toBe(true);
  await run("frameFontSizeOther");
  const input = within("font-scale").querySelector("input") as HTMLInputElement;
  fireEvent.change(input, { target: { value: "125" } });
  fireEvent.submit(input.form as HTMLFormElement);
  await vi.waitFor(() => expect(backend.setUiScale).toHaveBeenLastCalledWith(1.25));
  expect(bindings.frameFontSizeOther?.checked).toBe(true);
  expect(document.documentElement.style.zoom).toBe("1.25");
  await run("frameFontSizeSmall");
  expect(document.documentElement.style.zoom).toBe("");

  await run("playSound");
  expect(backend.setPlaySound).toHaveBeenLastCalledWith(true);
  backend.setAlwaysOnTop.mockRejectedValueOnce({ type: "WindowUnavailable", data: "main window is unavailable" });
  await run("alwaysOnTop");
  await vi.waitFor(() => expect(bindings.alwaysOnTop?.checked).toBe(true));
  expect((await screen.findByTestId("everyday-failed")).textContent).toContain("main window is unavailable");
});

function within(testId: string): HTMLElement {
  return screen.getByTestId(testId);
}

test("文件 → 加载棋谱中的贴目 follows the engine setting and changes it", async () => {
  const set = vi.fn();
  mount(everydayBackend(), VIEW, { value: false, set });
  expect(bindings.loadKomi?.checked).toBe(false);
  await run("loadKomi");
  expect(set).toHaveBeenCalledWith(true);
});

test("a stone sounds when one more move is played or stepped to, and only then", () => {
  const { rerender } = mount();
  rerender({ ...VIEW, board: board(19, 19, 4) });
  expect(sound.playStone).toHaveBeenCalledTimes(1);
  rerender({ ...VIEW, board: board(19, 19, 3) });
  rerender({ ...VIEW, board: board(19, 19, 10) });
  rerender({ ...VIEW, board: board(19, 19, 11, "Pass") });
  expect(sound.playStone).toHaveBeenCalledTimes(1);
  act(() => load({ play_sound: false } as Settings));
  rerender({ ...VIEW, board: board(19, 19, 12) });
  expect(sound.playStone).toHaveBeenCalledTimes(1);
});

test("Java's keys reach the new entries; the four new languages are offered", () => {
  const key = (code: string, mods: { ctrl?: boolean; alt?: boolean; shift?: boolean } = {}) =>
    matchShortcut({ key: "", code, ctrlKey: mods.ctrl ?? false, metaKey: false, altKey: mods.alt ?? false, shiftKey: mods.shift ?? false, repeat: false });
  expect(key("KeyC", { ctrl: true })).toBe("copySgf");
  expect(key("KeyV", { ctrl: true })).toBe("pasteSgf");
  expect(key("KeyC", { shift: true })).toBe("copyBoardScreen");
  expect(key("KeyC", { alt: true })).toBe("copySubBoardScreen");
  expect(key("KeyS", { ctrl: true, shift: true })).toBe("saveRaw");
  expect(key("KeyS", { ctrl: true, alt: true })).toBe("saveBranchRaw");
  expect(key("KeyS", { alt: true })).toBe("saveMainBoardScreen");
  expect(key("KeyS", { shift: true })).toBe("saveSubBoardScreen");
  expect(key("KeyS", { shift: true, alt: true })).toBe("saveWinrate");
  expect(key("KeyS")).toBe("saveAs");
  expect(key("KeyZ", { ctrl: true })).toBe("alwaysOnTop");
  expect(key("ArrowRight", { ctrl: true, alt: true })).toBe("spinRight");
  expect(key("ArrowDown", { ctrl: true, alt: true })).toBe("mirrorHorizon");
  expect(key("KeyS", { ctrl: true })).toBe("save");
  expect(key("ArrowDown", { ctrl: true })).toBe("forward10");
  expect([localeOf("ZhTw", "en"), localeOf("Ja", "en"), localeOf("Ko", "en"), localeOf("Th", "en")]).toEqual(["zh-TW", "ja", "ko", "th"]);
});
