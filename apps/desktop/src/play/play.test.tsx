import { act, cleanup, render } from "@testing-library/react";
import { useEffect } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { AnalysisView, Settings } from "@/api/backend";
import type { GameView } from "../api/game";
import en from "../locales/en.json";
import { isEnabled } from "../menu/bindings";
import { matchShortcut } from "../menu/shortcuts";
import { MENUS, withGtpEngines } from "../menu/spec";
import { aiForm, clockText, clockUrgent, gameForm, gameSetup, gtpTime, gtpTimeForm, joinArgs, kataGoPlay, MATCH_FORM, matchSetup, matchSideForm, resultText, splitArgs } from "./setup";
import { eloText, matchTitle } from "./shared";
import { usePlay } from "./usePlay";

afterEach(cleanup);
const labels = en.play;

describe("人机对局's form", () => {
  test("the default is Java's: you play Black against KataGo at 2 s a move, a new 19×19 board, komi 7.5", () => {
    const { game, ai } = gameForm(null);
    expect(gameSetup(game, ai)).toEqual({
      black: { type: "Human" },
      white: { type: "Ai", data: { type: "KataGo", data: { seconds: 2, visits: null, pure_network: false, resign: { after_moves: 150, consecutive: 2, below_winrate: 0.05 }, random_opening: null } } },
      start: { type: "NewBoard", data: expect.objectContaining({ width: 19, height: 19, komi: 7.5, handicap: 0, rules: "chinese" }) },
      human_clock: null,
      ponder: true,
      auto_save: true,
    });
  });

  test("a saved setup comes back as the dialog showed it, with the AI on the side it played", () => {
    const { game, ai } = gameForm(null);
    const saved = gameSetup({ ...game, human: "White", size: 13, komi: "6.5", clock: true, clockMain: "5", clockSeconds: "20", clockPeriods: "2" }, { ...ai, useVisits: true, visits: "400" });
    const again = gameForm(saved);
    expect(again.game).toMatchObject({ human: "White", size: 13, komi: "6.5", clock: true, clockMain: "5", clockSeconds: "20", clockPeriods: "2" });
    expect(again.ai).toMatchObject({ engine: "katago", useVisits: true, visits: "400" });
    expect(saved?.human_clock).toEqual({ main_seconds: 300, byoyomi_seconds: 20, periods: 2 });
    expect(saved?.black?.type).toBe("Ai");
  });

  test("the AI needs a limit; a clock needs some time; a bad number blocks the start", () => {
    const { game, ai } = gameForm(null);
    expect(gameSetup(game, { ...ai, useSeconds: false })).toBeNull();
    expect(gameSetup(game, { ...ai, useSeconds: false, pure: true })).not.toBeNull();
    expect(gameSetup({ ...game, clock: true, clockMain: "0", clockSeconds: "0" }, ai)).toBeNull();
    expect(gameSetup({ ...game, komi: "seven" }, ai)).toBeNull();
    expect(kataGoPlay({ ...ai, resignBelow: "12.5" })?.resign?.below_winrate).toBe(0.125);
  });

  test("当前局面续弈 keeps the tree; free handicap is for GTP engines only", () => {
    const { game, ai } = gameForm(null);
    expect(gameSetup({ ...game, continueHere: true }, ai)?.start).toEqual({ type: "CurrentPosition" });
    expect(gameSetup({ ...game, handicap: 4, free: true }, ai)?.start).toMatchObject({ type: "NewBoard", data: { handicap: 0 } });
    const gtp = { ...ai, engine: "gtp-1", gtp: gtpTimeForm({ type: "PerMove", data: 5 }) };
    expect(gameSetup({ ...game, handicap: 4, free: true }, gtp)?.start).toMatchObject({ type: "FreeHandicap", data: { stones: 4 } });
    expect(gameSetup(game, gtp)?.white).toEqual({ type: "Ai", data: { type: "Gtp", data: { engine_id: "gtp-1", time: { type: "PerMove", data: 5 } } } });
  });

  test("GTP time: minutes typed, seconds sent; Fischer and sudden death need time; custom lines are trimmed", () => {
    const form = gtpTimeForm();
    expect(gtpTime({ ...form, mode: "Byoyomi", main: "10", period: "30", stones: "1" })).toEqual({ type: "Byoyomi", data: { main: 600, period: 30, stones: 1 } });
    expect(gtpTime({ ...form, mode: "Fischer", main: "0", increment: "0" })).toBeNull();
    expect(gtpTime({ ...form, mode: "Absolute", main: "1.5" })).toEqual({ type: "Absolute", data: { main: 90 } });
    expect(gtpTime({ ...form, mode: "Custom", custom: " time_settings 0 5 1 \n\n" })).toEqual({ type: "Custom", data: ["time_settings 0 5 1"] });
    expect(gtpTimeForm(gtpTime({ ...form, mode: "Byoyomi", main: "2" }))).toMatchObject({ mode: "Byoyomi", main: "2" });
  });
});

describe("引擎对局", () => {
  test("each side is a KataGo entry with its limits and WRN/PDA, or a GTP entry with its time", () => {
    const a = matchSideForm("k:main");
    const b = { ...matchSideForm("g:gtp-1"), ai: { ...aiForm(null), engine: "g:gtp-1" } };
    const setup = matchSetup({ ...MATCH_FORM, games: "10", limitMoves: true, maxMoves: "300", openings: ["/a.sgf"] }, { ...a, wrn: "0.5", pda: "-1" }, b);
    expect(setup).toMatchObject({
      a: { type: "KataGo", data: { engine_id: "main", wrn: 0.5, pda: -1, play: { resign: { after_moves: 0, consecutive: 2, below_winrate: 0.1 } } } },
      b: { type: "Gtp", data: { engine_id: "gtp-1", time: { type: "PerMove", data: 2 } } },
      games: 10,
      swap_colors: true,
      max_moves: 300,
      openings: ["/a.sgf"],
      random_openings: true,
      auto_save: true,
    });
    expect(matchSetup(MATCH_FORM, { ...a, pda: "4" }, b)).toBeNull();
    expect(matchSetup({ ...MATCH_FORM, games: "0" }, a, b)).toBeNull();
    expect(matchSetup(MATCH_FORM, a, b)?.max_moves).toBeNull();
  });

  test("the score reads as Java's title, and Elo shows its 2σ range or nothing yet", () => {
    const view = { a: { total: 3 }, b: { total: 1 }, draws: 0, elo: { elo: 190.8, lower: -40.2, upper: null } };
    expect(matchTitle(view, ["A", "B"], en.match)).toBe("3:1 A VS B winrate 75%");
    expect(eloText(view, en.match)).toBe("191 (2σ -40 to –)");
    expect(eloText({}, en.match)).toBe("Not yet");
  });
});

test("results, clocks and engine arguments read as people write them", () => {
  expect(resultText("B+R", labels)).toBe("Black wins · resignation");
  expect(resultText("W+T", labels)).toBe("White wins · time");
  expect(resultText("B+3.5", labels)).toBe("Black wins by 3.5");
  expect(resultText("0", labels)).toBe("Draw");
  expect(clockText({ main_seconds: 75.2, periods: 3, byoyomi_seconds: 30 }, labels)).toBe("1:16");
  expect(clockText({ main_seconds: 0, periods: 2, byoyomi_seconds: 8.4 }, labels)).toBe("9s ×2");
  expect(clockUrgent({ main_seconds: 0, periods: 2, byoyomi_seconds: 8 })).toBe(true);
  expect(clockUrgent({ main_seconds: 8, periods: 2, byoyomi_seconds: 30 })).toBe(false);
  const args = ["gtp", "-config", "/My Engines/zen.cfg", ""];
  expect(splitArgs(joinArgs(args))).toEqual(args);
  expect(splitArgs(`--weights 'a b.gz'  -t 4`)).toEqual(["--weights", "a b.gz", "-t", "4"]);
});

test("Java's keys: N, Alt+N, Alt+E and Enter; 设置 lists GTP 引擎… right after 引擎设置", () => {
  const key = (code: string, alt = false) => matchShortcut({ key: "", code, ctrlKey: false, metaKey: false, altKey: alt, shiftKey: false, repeat: false });
  expect([key("KeyN"), key("KeyN", true), key("KeyE", true), key("Enter")]).toEqual(["newGenmoveGame", "newAnalyzeModeGame", "newEngineGame", "continueGameHere"]);
  const settings = withGtpEngines(MENUS, "GTP engines…").find((menu) => menu.id === "settings")!;
  const ids = settings.items.map((entry) => (entry.kind === "separator" ? "-" : entry.id));
  expect(ids.slice(0, 2)).toEqual(["engineConfig", "gtpEngines"]);
});

describe("usePlay", () => {
  type Listener<T> = (value: T) => void;
  let gameListener: Listener<GameView> | null;
  let backend: ReturnType<typeof makeBackend>;
  function makeBackend() {
    return {
      gameView: vi.fn(() => Promise.resolve(null)),
      subscribeGameUpdated: vi.fn((listener: Listener<GameView>) => {
        gameListener = listener;
        return Promise.resolve(1);
      }),
      subscribeMatchUpdated: vi.fn(() => Promise.resolve(2)),
      subscribeGtpConsole: vi.fn(() => Promise.resolve(3)),
      unsubscribe: vi.fn(() => Promise.resolve()),
      continueGame: vi.fn((color?: "Black" | "White" | null) => Promise.resolve<GameView>({ phase: "Playing", to_play: color === "White" ? "Black" : "White" })),
      playBestMove: vi.fn(() => Promise.resolve(4)),
      pickOpenPaths: vi.fn(() => Promise.resolve([])),
      pickDirectory: vi.fn(() => Promise.resolve(null)),
      pickEngineFile: vi.fn(() => Promise.resolve(null)),
    };
  }
  type Hook = ReturnType<typeof usePlay>;
  let latest: Hook | null = null;
  function Harness({ hasStones = true }: { hasStones?: boolean }) {
    const hook = usePlay(backend as never, { version: 1, engine: null, pondering: false, gtp_engines: [{ id: "gtp-1", name: "Zen" }] } as Settings, vi.fn(), { hasDocument: true, hasStones }, en);
    useEffect(() => {
      latest = hook;
    });
    return <>{hook.dialogs()}</>;
  }
  beforeEach(() => {
    latest = null;
    gameListener = null;
    backend = makeBackend();
  });

  test("the game entries open, continue and steer the game; a finished game shows its result", async () => {
    const view = render(<Harness />);
    await act(async () => undefined);
    let bindings = latest!.bindings(null, null);
    expect(isEnabled(bindings.newGenmoveGame)).toBe(true);
    expect(isEnabled(bindings.setAiTime)).toBe(false);
    expect(bindings.playBestMove).toBeUndefined();
    await act(async () => bindings.continueGenmoveGameAsWhite?.run?.());
    expect(backend.continueGame).toHaveBeenLastCalledWith("White");
    await act(async () => latest!.bindings(null, null).continueGameHere?.run?.());
    expect(backend.continueGame).toHaveBeenLastCalledWith(null);
    bindings = latest!.bindings(null, null);
    expect(latest!.active).toBe(true);
    expect(isEnabled(bindings.newGenmoveGame)).toBe(false);
    expect(isEnabled(bindings.setAiTime)).toBe(true);
    act(() => gameListener?.({ phase: "Finished", result: "W+R", end_reason: "Resignation" }));
    expect(view.getByTestId("play-result").textContent).toBe("White wins · resignation");
    expect(latest!.active).toBe(false);
  });

  test("对弈模式 hides the candidates for the side that asked not to see them", async () => {
    render(<Harness />);
    await act(async () => undefined);
    act(() => gameListener?.({ phase: "Playing", to_play: "Black" }));
    expect(latest!.hideCandidates).toBe(false);
    expect(latest!.shown({ show_winrate_graph: true } as never)).toEqual({ show_winrate_graph: true });
  });

  test("with a candidate under the pointer, 让AI落子最佳一手 plays its whole line", async () => {
    render(<Harness />);
    await act(async () => undefined);
    const analysis = {
      candidates: [
        { order: 1, point: { x: 3, y: 3 }, pv: [{}, {}] },
        { order: 0, point: { x: 15, y: 3 }, pv: [{}, {}, {}] },
      ],
    } as unknown as AnalysisView;
    await act(async () => latest!.bindings(analysis, { x: 3, y: 3 }).playBestMove?.run?.());
    expect(backend.playBestMove).toHaveBeenCalledWith({ candidate: 1, moves: 2 });
  });
});
