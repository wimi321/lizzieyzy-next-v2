import { beforeEach, expect, test, vi } from "vitest";
import { defaultGameSetup, defaultGameView } from "../../e2e/fixtures";
import type { GameView } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));
beforeEach(() => { vi.resetModules(); ipc.invoke.mockReset(); });

test("game commands forward typed inputs and preserve errors", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValue(defaultGameView);
  await backend.startGame(defaultGameSetup);
  expect(ipc.invoke).toHaveBeenLastCalledWith("start_game", { setup: defaultGameSetup });
  await backend.pauseGame();
  expect(ipc.invoke).toHaveBeenLastCalledWith("pause_game");
  await backend.resumeGame();
  expect(ipc.invoke).toHaveBeenLastCalledWith("resume_game");
  await backend.stopGame();
  expect(ipc.invoke).toHaveBeenLastCalledWith("stop_game");
  await backend.resignGame();
  expect(ipc.invoke).toHaveBeenLastCalledWith("resign_game");
  await backend.takeBack();
  expect(ipc.invoke).toHaveBeenLastCalledWith("take_back");
  await backend.continueGame();
  expect(ipc.invoke).toHaveBeenLastCalledWith("continue_game", { aiColor: null });
  await backend.continueGame("Black");
  expect(ipc.invoke).toHaveBeenLastCalledWith("continue_game", { aiColor: "Black" });
  const player = { type: "KataGo" as const, data: { visits: 77, seconds: null } };
  await backend.setAiPlayer("White", player);
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_ai_player", { color: "White", player });
  await backend.playBestMove();
  expect(ipc.invoke).toHaveBeenLastCalledWith("play_best_move", { pv: null });
  await backend.playBestMove({ candidate: 1, moves: 3 });
  expect(ipc.invoke).toHaveBeenLastCalledWith("play_best_move", { pv: { candidate: 1, moves: 3 } });
  await backend.gameView();
  expect(ipc.invoke).toHaveBeenLastCalledWith("game_view");
  const error = { type: "Game", data: { type: "NoAnalysis" } };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.playBestMove()).rejects.toBe(error);
});

test("mock game commands require scripts and game events use a separate listener", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const received: GameView[] = [];
  mock.setScript({ commands: [
    { command: "startGame", result: { phase: "Playing" }, events: [{ type: "GameUpdated", data: { phase: "Playing", move_number: 3 } }] },
    { command: "pauseGame", result: { phase: "Paused" } },
    { command: "resumeGame", result: { phase: "Playing" } },
    { command: "takeBack", result: { phase: "Playing", move_number: 1 } },
    { command: "resignGame", result: { phase: "Finished", result: "W+R" } },
    { command: "continueGame", result: { phase: "Playing" } },
    { command: "setAiPlayer", result: { phase: "Playing" } },
    { command: "playBestMove", result: 42 },
    { command: "stopGame", result: { phase: "Finished", end_reason: "Stopped" } },
  ] });
  const oldListener = vi.fn();
  const old = await mock.subscribe(oldListener);
  const game = await mock.subscribeGameUpdated((view) => received.push(view));
  expect(await mock.gameView()).toBeNull();
  const started = await mock.startGame({});
  expect(started.black).toEqual(defaultGameSetup.black);
  expect(received[0]?.move_number).toBe(3);
  expect(received[0]?.ai_thinking).toBe(false);
  expect(oldListener).not.toHaveBeenCalled();
  expect((await mock.pauseGame()).phase).toBe("Paused");
  await mock.resumeGame();
  await mock.takeBack();
  await mock.resignGame();
  await mock.continueGame();
  await mock.setAiPlayer("White", { type: "KataGo", data: {} });
  expect(await mock.playBestMove({ candidate: 0, moves: 2 })).toBe(42);
  await mock.stopGame();
  expect((await mock.gameView())?.end_reason).toBe("Stopped");
  await expect(mock.startGame({})).rejects.toThrow("Expected mock command (none)");
  await mock.unsubscribe(old); await mock.unsubscribe(game);
});

test("mock game setting defaults preserve explicit limits and directory", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "settings", result: { version: 1, engine: null, pondering: true } },
    { command: "settings", result: { version: 1, engine: null, pondering: true, games_directory: "custom", game_setup: { ponder: false, human_clock: {}, white: { type: "Ai", data: { type: "KataGo", data: { seconds: null, visits: 7, resign: null } } } } } },
  ] });
  const old = await mock.settings();
  expect(old.game_setup).toEqual(defaultGameSetup);
  expect(old.games_directory).toBeNull();
  const explicit = await mock.settings();
  expect(explicit.games_directory).toBe("custom");
  expect(explicit.game_setup?.ponder).toBe(false);
  expect(explicit.game_setup?.human_clock?.main_seconds).toBe(1200);
  const white = explicit.game_setup?.white;
  expect(white?.type === "Ai" && white.data.type === "KataGo" && white.data.data.seconds).toBeNull();
  expect(white?.type === "Ai" && white.data.type === "KataGo" && white.data.data.visits).toBe(7);
  expect(white?.type === "Ai" && white.data.type === "KataGo" && white.data.data.resign).toBeNull();
});
