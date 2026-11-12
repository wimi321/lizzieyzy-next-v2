import { beforeEach, expect, test, vi } from "vitest";

const dialogs = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialogs);

beforeEach(() => {
  vi.resetModules();
  dialogs.open.mockReset();
  dialogs.save.mockReset();
});

test("opening selects SGF or GIB while saving selects SGF only", async () => {
  const backend = await import("./backend");
  dialogs.open.mockResolvedValueOnce("棋谱.GIB");
  expect(await backend.pickOpenPath()).toBe("棋谱.GIB");
  expect(dialogs.open).toHaveBeenCalledWith({
    multiple: false, directory: false,
    filters: [{ name: "SGF / GIB", extensions: ["sgf", "gib"] }],
  });
  dialogs.save.mockResolvedValueOnce("棋谱.sgf");
  expect(await backend.pickSavePath()).toBe("棋谱.sgf");
  expect(dialogs.save).toHaveBeenCalledWith({
    filters: [{ name: "SGF", extensions: ["sgf"] }],
  });
});

test("cancelling either file dialog preserves cancellation", async () => {
  const backend = await import("./backend");
  dialogs.open.mockResolvedValue(null);
  dialogs.save.mockResolvedValue(null);
  expect(await backend.pickOpenPath()).toBeNull();
  expect(await backend.pickSavePath()).toBeNull();
});
