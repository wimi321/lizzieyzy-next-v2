import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useEffect } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { BatchAnalysisProgressListener, BatchAnalysisStatus, BatchMove, BatchOptions, FlashAnalysisSettings, Settings, WholeGameOptions } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { isEnabled, type MenuBinding, type MenuBindings } from "../menu/bindings";
import type { WholeGameControls } from "../review/useWholeGame";
import { rangeScope } from "./range";
import { useBatch, type BatchView } from "./useBatch";

afterEach(cleanup);

const idle: BatchAnalysisStatus = { phase: "Idle", items: [], current_index: null, reason: null };

function batchBackend() {
  let listener: BatchAnalysisProgressListener = () => {};
  const status = (next: Partial<BatchAnalysisStatus>) => Promise.resolve<BatchAnalysisStatus>({ ...idle, ...next });
  const backend = {
    startBatchAnalysis: vi.fn<(options: BatchOptions) => Promise<BatchAnalysisStatus>>(() => status({ phase: "Running" })),
    pauseBatchAnalysis: vi.fn(() => status({ phase: "Paused" })),
    resumeBatchAnalysis: vi.fn(() => status({ phase: "Running" })),
    stopBatchAnalysis: vi.fn(() => status({ phase: "Stopped" })),
    addBatchAnalysisFiles: vi.fn<(files: string[]) => Promise<BatchAnalysisStatus>>(() => status({})),
    removeBatchAnalysisItem: vi.fn<(id: number) => Promise<BatchAnalysisStatus>>(() => status({})),
    moveBatchAnalysisItem: vi.fn<(id: number, direction: BatchMove) => Promise<BatchAnalysisStatus>>(() => status({})),
    clearBatchAnalysis: vi.fn(() => status({})),
    batchAnalysisStatus: vi.fn(() => status({})),
    subscribeBatchAnalysisProgress: vi.fn((next: BatchAnalysisProgressListener) => {
      listener = next;
      return Promise.resolve(1);
    }),
    unsubscribe: vi.fn(() => Promise.resolve()),
    setFlashAnalysis: vi.fn((settings: FlashAnalysisSettings) => Promise.resolve(settings)),
    pickOpenPaths: vi.fn(() => Promise.resolve(["/games/一.sgf", "/games/二.gib"])),
    pickDirectory: vi.fn(() => Promise.resolve<string | null>("/out")),
    openPath: vi.fn<(path: string) => Promise<number>>(() => Promise.resolve(3)),
  };
  return { backend, emit: (next: BatchAnalysisStatus) => act(() => listener(next)) };
}

function controls(): WholeGameControls {
  return { progress: null, start: vi.fn(), run: vi.fn<(options: WholeGameOptions) => void>(), pause: vi.fn(), resume: vi.fn(), cancel: vi.fn() };
}

type Backend = ReturnType<typeof batchBackend>["backend"];
let latest: ReturnType<typeof useBatch> | null = null;
function expose(batch: ReturnType<typeof useBatch>) {
  latest = batch;
}

function Harness({ backend, whole, view }: { backend: Backend; whole: WholeGameControls; view: BatchView }) {
  const batch = useBatch(backend, whole, view, zh.batch);
  useEffect(() => expose(batch));
  return (
    <>
      {batch.tray()}
      {batch.dialogs(zh.review)}
    </>
  );
}

const VIEW: BatchView = { hasDocument: true, modified: false };
let setup: ReturnType<typeof batchBackend>;
let whole: WholeGameControls;
beforeEach(() => {
  latest = null;
  setup = batchBackend();
  whole = controls();
});

function mount(view: BatchView = VIEW) {
  const result = render(<Harness backend={setup.backend} whole={whole} view={view} />);
  return (next: BatchView) => result.rerender(<Harness backend={setup.backend} whole={whole} view={next} />);
}
const binding = (id: string): MenuBinding | undefined => latest?.bindings[id];
const run = (id: string) => act(() => binding(id)?.run?.());

const running: BatchAnalysisStatus = {
  phase: "Running",
  current_index: 1,
  reason: null,
  items: [
    { id: 0, path: "/games/一.sgf", state: { type: "Completed", data: { output: "/games/一_已分析_20261113120000.sgf" } } },
    { id: 1, path: "/games/二.sgf", state: { type: "Running", data: { progress: { completed: 30, total: 120, visits: 1000, status: { type: "Running" } } } } },
    { id: 2, path: "/games/三.sgf", state: { type: "Failed", data: { reason: "file is not a game record" } } },
    { id: 3, path: "/games/四.sgf", state: { type: "Waiting" } },
  ],
};

test("批量分析 picks games, asks how to analyse them and starts a batch whose copies are named in the interface language", async () => {
  mount();
  await run("batchAnalyze");
  const dialog = await screen.findByTestId("batch-dialog");
  expect(screen.getByTestId("batch-files").textContent).toContain("2 盘棋谱");
  fireEvent.click(screen.getByTestId("batch-deep"));
  fireEvent.click(screen.getByTestId("batch-scope-range"));
  fireEvent.change(screen.getByTestId("batch-first"), { target: { value: "50" } });
  fireEvent.change(screen.getByTestId("batch-last"), { target: { value: "20" } });
  expect((screen.getByTestId("batch-start") as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByTestId("batch-last"), { target: { value: "" } });
  fireEvent.click(screen.getByTestId("batch-choose-folder"));
  await vi.waitFor(() => expect(screen.getByTestId("batch-folder").textContent).toBe("/out"));
  fireEvent.click(screen.getByTestId("batch-start"));
  await vi.waitFor(() => expect(setup.backend.startBatchAnalysis).toHaveBeenCalledTimes(1));
  expect(setup.backend.startBatchAnalysis).toHaveBeenCalledWith({
    files: ["/games/一.sgf", "/games/二.gib"],
    visits: 3000,
    scope: { type: "Range", data: { first_move: 50, last_move: null } },
    force: false,
    output: { suffix: "已分析", directory: "/out" },
  });
  expect(dialog.isConnected).toBe(false);
  expect(screen.getByTestId("batch-table")).toBeTruthy();
});

test("批量精析 takes visits and overwriting from 精析设置; unsaved changes are confirmed first", async () => {
  mount({ hasDocument: true, modified: true });
  act(() => latest?.load({ flash_analysis: { visits: 50, always_override: true } } as Settings));
  await run("batchAnalysisMode");
  expect((await screen.findByTestId("batch-flash-visits")).textContent).toContain("单步计算量 50");
  fireEvent.click(screen.getByTestId("batch-scope-branches"));
  fireEvent.click(screen.getByTestId("batch-start"));
  expect(setup.backend.startBatchAnalysis).not.toHaveBeenCalled();
  fireEvent.click(screen.getByTestId("batch-unsaved-ok"));
  await vi.waitFor(() => expect(setup.backend.startBatchAnalysis).toHaveBeenCalledTimes(1));
  expect(setup.backend.startBatchAnalysis.mock.calls[0]?.[0]).toMatchObject({ visits: 50, scope: { type: "AllBranches" }, force: true, output: { directory: null } });
});

test("the progress table shows each game's state and steers the queue; the tray shows the batch", async () => {
  mount();
  await run("batchAnalyzeTable");
  setup.emit(running);
  const rows = screen.getAllByTestId("batch-row");
  expect(rows.map((row) => row.getAttribute("data-state"))).toEqual(["Completed", "Running", "Failed", "Waiting"]);
  expect(rows[1]?.textContent).toContain("分析中 30/120");
  expect(rows[2]?.textContent).toContain("失败：file is not a game record");
  expect(screen.getByTestId("batch-chip").textContent).toBe("批量 2/4");

  // Only a waiting game can be moved or removed.
  fireEvent.click(rows[2] as HTMLElement);
  expect((screen.getByTestId("batch-up") as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(rows[3] as HTMLElement);
  fireEvent.click(screen.getByTestId("batch-first"));
  fireEvent.click(screen.getByTestId("batch-remove"));
  expect(setup.backend.moveBatchAnalysisItem).toHaveBeenCalledWith(3, "First");
  expect(setup.backend.removeBatchAnalysisItem).toHaveBeenCalledWith(3);
  // Analysed copies open only once the batch is over.
  expect((screen.getByTestId("batch-open-output") as HTMLButtonElement).disabled).toBe(true);

  fireEvent.click(screen.getByTestId("batch-pause"));
  await vi.waitFor(() => expect(setup.backend.pauseBatchAnalysis).toHaveBeenCalledTimes(1));
  fireEvent.click(screen.getByTestId("batch-add"));
  await vi.waitFor(() => expect(setup.backend.addBatchAnalysisFiles).toHaveBeenCalledWith(["/games/一.sgf", "/games/二.gib"]));

  setup.emit({ ...running, phase: "Completed", current_index: null, items: running.items?.slice(0, 1) ?? [] });
  fireEvent.click(screen.getByTestId("batch-open-output"));
  expect(setup.backend.openPath).toHaveBeenCalledWith("/games/一_已分析_20261113120000.sgf");
  expect(screen.queryByTestId("batch-chip")).toBeNull();
});

test("a running batch answers what it refuses with an explanation, and the whole-game controls steer it", async () => {
  mount();
  const all: MenuBindings = { open: { run: vi.fn() }, recent0: { run: vi.fn() }, readBoard: { enabled: false, run: vi.fn() }, nextMove: { run: vi.fn() } };
  expect(latest?.guard(all)).toBe(all);
  setup.emit(running);
  const guarded = latest?.guard(all) ?? {};
  expect(guarded.nextMove).toBe(all.nextMove);
  expect(isEnabled(guarded.readBoard)).toBe(false);
  act(() => guarded.open?.run?.());
  expect(all.open?.run).not.toHaveBeenCalled();
  expect((await screen.findByTestId("batch-message")).textContent).toContain("批量分析进行中");
  fireEvent.click(screen.getByTestId("batch-message-ok"));

  await run("flashAnalyzeAllBranches");
  expect(whole.run).not.toHaveBeenCalled();
  fireEvent.click(await screen.findByTestId("batch-message-ok"));

  act(() => latest?.controls.pause());
  await vi.waitFor(() => expect(setup.backend.pauseBatchAnalysis).toHaveBeenCalledTimes(1));
  expect(whole.pause).not.toHaveBeenCalled();
  setup.emit(running);
  await run("stopAutoAnalyze");
  await vi.waitFor(() => expect(setup.backend.stopBatchAnalysis).toHaveBeenCalledTimes(1));
  expect(whole.cancel).not.toHaveBeenCalled();
});

test("精析 entries use 精析设置; 局部精析 takes a range; 确定并重新计算 redoes the last one", async () => {
  mount();
  act(() => latest?.load({ flash_analysis: { visits: 200, always_override: false } } as Settings));
  await run("flashAnalyzeAllGame");
  expect(whole.run).toHaveBeenLastCalledWith({ visits: 200, scope: { type: "Line" }, force: false });
  await run("flashAnalyzePartGame");
  fireEvent.change(await screen.findByTestId("flash-first"), { target: { value: "30" } });
  fireEvent.change(screen.getByTestId("flash-last"), { target: { value: "60" } });
  fireEvent.click(screen.getByTestId("flash-range-start"));
  expect(whole.run).toHaveBeenLastCalledWith({ visits: 200, scope: { type: "Range", data: { first_move: 30, last_move: 60 } }, force: false });

  await run("flashAnalyzeSettings");
  fireEvent.change(await screen.findByTestId("flash-visits"), { target: { value: "0" } });
  expect((screen.getByTestId("flash-settings-save") as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByTestId("flash-visits"), { target: { value: "800" } });
  fireEvent.click(screen.getByTestId("flash-override"));
  fireEvent.click(screen.getByTestId("flash-settings-redo"));
  await vi.waitFor(() => expect(setup.backend.setFlashAnalysis).toHaveBeenCalledWith({ visits: 800, always_override: true }));
  await vi.waitFor(() =>
    expect(whole.run).toHaveBeenLastCalledWith({ visits: 800, scope: { type: "Range", data: { first_move: 30, last_move: 60 } }, force: true }),
  );
  await act(async () => {});
  await run("flashAnalyzeAllBranches");
  expect(whole.run).toHaveBeenLastCalledWith({ visits: 800, scope: { type: "AllBranches" }, force: true });
});

test("a refused command is explained", async () => {
  mount();
  setup.backend.startBatchAnalysis.mockRejectedValueOnce({ type: "Batch", data: { type: "InvalidSuffix" } });
  await run("batchAnalyze");
  fireEvent.click(await screen.findByTestId("batch-start"));
  expect((await screen.findByTestId("batch-message")).textContent).toContain("InvalidSuffix");
});

test("a move range may leave either end blank but not run backwards", () => {
  expect(rangeScope("", "")).toEqual({ type: "Range", data: { first_move: null, last_move: null } });
  expect(rangeScope("12", "")).toEqual({ type: "Range", data: { first_move: 12, last_move: null } });
  expect(rangeScope("0", "5")).toBeNull();
  expect(rangeScope("9", "3")).toBeNull();
  expect(rangeScope("a", "")).toBeNull();
});
