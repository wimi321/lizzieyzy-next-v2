import { useCallback, useEffect, useRef, useState } from "react";
import type { AnalysisView, Backend, Settings } from "@/api/backend";
import type { AiPlayer, GameSetup, GameView, PvPick } from "../api/game";
import type { GtpConsoleLine, GtpEngineEntry } from "../api/gtp";
import type { MatchSetup, MatchView } from "../api/match-game";
import type { Display } from "../display/settings";
import { reasonOf } from "../everyday/useEveryday";
import { ConfirmDialog } from "../frame/ConfirmDialog";
import { MessageBox } from "../frame/MessageBox";
import type { MenuBindings } from "../menu/bindings";
import type en from "../locales/en.json";
import { GtpConsole, GtpEngines } from "./GtpEngines";
import { type KataGoEntry, MatchDialog } from "./MatchDialog";
import { GamesDialog, MatchPanel } from "./MatchPanel";
import { AiTimeDialog, type FocusOptions, GameNotice, PlayDialog } from "./PlayDialog";
import { clockText, clockUrgent } from "./setup";
import { matchTitle } from "./shared";

type PlayBackend = Pick<
  Backend,
  | "startGame"
  | "pauseGame"
  | "resumeGame"
  | "stopGame"
  | "resignGame"
  | "takeBack"
  | "continueGame"
  | "setAiPlayer"
  | "playBestMove"
  | "gameView"
  | "subscribeGameUpdated"
  | "startMatch"
  | "pauseMatch"
  | "resumeMatch"
  | "stopMatch"
  | "setMatchGames"
  | "matchPlayNow"
  | "matchResign"
  | "setMatchSideParams"
  | "subscribeMatchUpdated"
  | "addGtpEngine"
  | "updateGtpEngine"
  | "removeGtpEngine"
  | "testGtpEngine"
  | "gtpConsole"
  | "gtpConsoleSend"
  | "subscribeGtpConsole"
  | "unsubscribe"
  | "pickOpenPaths"
  | "pickDirectory"
  | "pickEngineFile"
>;

type Messages = Pick<typeof en, "play" | "match" | "gtp" | "info">;

export interface PlayView {
  hasDocument: boolean;
  /** The board has stones: 当前局面续弈 has something to continue. */
  hasStones: boolean;
}

/** The in-game buttons of the top toolbar's 对局 group. */
export interface GameToolbar {
  active: boolean;
  paused: boolean;
  /** 新对局 opens the dialog; busy while an engine match runs. */
  canStart: boolean;
  onNew(): void;
  onPause(): void;
  onResume(): void;
  onTakeBack(): void;
  onResign(): void;
  onEnd(): void;
  labels: { resume: string; takeBack: string; resignGame: string; endGame: string };
}

type Dialog = "play" | "aiTime" | "match" | "games" | "gtp";
const NO_FOCUS: FocusOptions = { focus: false, showBlack: false, showWhite: false };
const CONSOLE_LIMIT = 2000;

/**
 * P3-11: 人机对局, 人机续弈, 修改AI用时, 引擎对局 and its score, GTP 引擎 and the Gtp控制台.
 * Rust owns the game, the clocks and the match; this shows them and sends commands.
 */
export function usePlay(backend: PlayBackend, settings: Settings | null, applySettings: (settings: Settings) => void, view: PlayView, messages: Messages) {
  const labels = messages.play;
  const [game, setGame] = useState<GameView | null>(null);
  const [match, setMatch] = useState<MatchView | null>(null);
  const [sides, setSides] = useState<{ names: [string, string]; katago: [boolean, boolean] }>({ names: ["1", "2"], katago: [false, false] });
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [panelOpen, setPanelOpen] = useState(false);
  const [consoleOpen, setConsoleOpen] = useState(false);
  const [consoleEngine, setConsoleEngine] = useState<string | null>(null);
  const [consoleLines, setConsoleLines] = useState<GtpConsoleLine[]>([]);
  const [focus, setFocus] = useState<FocusOptions>(NO_FOCUS);
  const [notice, setNotice] = useState<GameView | null>(null);
  const [confirm, setConfirm] = useState<"resign" | "end" | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [lastSetup, setLastSetup] = useState<GameSetup | null>(null);
  const [busy, setBusy] = useState(false);
  const phase = useRef<GameView["phase"] | null>(null);
  // Rust's events are the truth: a command's reply only counts if no event came after it was sent.
  const gameEvents = useRef(0);
  const matchEvents = useRef(0);

  const gtpEngines: GtpEngineEntry[] = settings?.gtp_engines ?? [];
  const katagoEngines: KataGoEntry[] = (settings?.engines ?? []).map((entry, index) => ({ id: entry.id ?? String(index), name: entry.name || `KataGo ${index + 1}` }));

  const describe = useCallback(
    (error: unknown): string => {
      if (typeof error === "object" && error !== null && "type" in error) {
        const outer = error as { type: string; data?: unknown };
        const inner = outer.type === "Game" && typeof outer.data === "object" && outer.data !== null ? (outer.data as { type: string }) : outer;
        const known = labels.errors[inner.type as keyof typeof labels.errors];
        if (known) return known;
      }
      return reasonOf(error);
    },
    [labels],
  );

  const showGame = useCallback((next: GameView | null) => {
    setGame(next);
    if (next?.phase === "Finished" && phase.current !== null && phase.current !== "Finished") setNotice(next);
    phase.current = next?.phase ?? null;
  }, []);

  useEffect(() => {
    let active = true;
    const ids: Promise<number>[] = [];
    void backend
      .gameView?.()
      .then((current) => {
        if (active && current) showGame(current);
      })
      .catch(() => undefined);
    const onGame = (next: GameView) => {
      if (!active) return;
      gameEvents.current += 1;
      showGame(next);
    };
    const onMatch = (next: MatchView) => {
      if (!active) return;
      matchEvents.current += 1;
      setMatch(next);
    };
    if (backend.subscribeGameUpdated) ids.push(backend.subscribeGameUpdated(onGame));
    if (backend.subscribeMatchUpdated) ids.push(backend.subscribeMatchUpdated(onMatch));
    if (backend.subscribeGtpConsole) ids.push(backend.subscribeGtpConsole((line) => active && setConsoleLines((lines) => [...lines, line].slice(-CONSOLE_LIMIT))));
    return () => {
      active = false;
      for (const id of ids) void id.then((value) => backend.unsubscribe(value)).catch(() => undefined);
    };
  }, [backend, showGame]);

  const active = game?.phase === "Playing" || game?.phase === "Paused";
  const matchActive = match?.phase === "Running" || match?.phase === "Paused";
  const report = (error: unknown) => setFailure(describe(error));
  const replyGame = (sent: number) => (next: GameView | null) => {
    if (next && gameEvents.current === sent) showGame(next);
  };
  const replyMatch = (sent: number) => (next: MatchView) => {
    if (matchEvents.current === sent) setMatch(next);
  };
  const run = (action: (() => Promise<GameView>) | undefined) => {
    if (!action) return;
    void action()
      .then(replyGame(gameEvents.current))
      .catch(report);
  };
  const runMatch = (action: (() => Promise<MatchView>) | undefined) => {
    if (!action) return;
    void action()
      .then(replyMatch(matchEvents.current))
      .catch(report);
  };

  const startGame = async (setup: GameSetup, chosen: FocusOptions): Promise<string | null> => {
    if (!backend.startGame) return labels.errors.NoEngine;
    try {
      replyGame(gameEvents.current)(await backend.startGame(setup));
      setLastSetup(setup);
      setFocus(chosen);
      setDialog(null);
      return null;
    } catch (error) {
      return describe(error);
    }
  };

  const continueGame = (color: "Black" | "White" | null) => run(backend.continueGame && (() => backend.continueGame!(color)));

  const setAiPlayers = async (changes: readonly ["Black" | "White", AiPlayer][]): Promise<string | null> => {
    try {
      for (const [color, player] of changes) {
        replyGame(gameEvents.current)((await backend.setAiPlayer?.(color, player)) ?? null);
      }
      return null;
    } catch (error) {
      return describe(error);
    }
  };

  const startMatch = async (setup: MatchSetup): Promise<string | null> => {
    if (!backend.startMatch) return labels.errors.NoEngine;
    const nameOf = (player: MatchSetup["a"]) =>
      player?.type === "Gtp"
        ? (gtpEngines.find((entry) => entry.id === player.data.engine_id)?.name ?? player.data.engine_id)
        : (katagoEngines.find((entry) => entry.id === player?.data.engine_id)?.name ?? "KataGo");
    try {
      const sent = matchEvents.current;
      const started = await backend.startMatch(setup);
      setSides({ names: [nameOf(setup.a), nameOf(setup.b)], katago: [setup.a?.type === "KataGo", setup.b?.type === "KataGo"] });
      replyMatch(sent)(started);
      setPanelOpen(true);
      setDialog(null);
      return null;
    } catch (error) {
      return describe(error);
    }
  };

  const saveGtp = async (entry: GtpEngineEntry): Promise<string | null> => {
    const action = entry.id ? backend.updateGtpEngine : backend.addGtpEngine;
    if (!action) return labels.errors.NoEngine;
    setBusy(true);
    try {
      applySettings(await action(entry));
      return null;
    } catch (error) {
      return describe(error);
    } finally {
      setBusy(false);
    }
  };

  const testGtp = async (id: string) => {
    try {
      const info = await backend.testGtpEngine!(id);
      return { ok: true, text: messages.gtp.testOk.replace("{name}", info.name ?? "").replace("{version}", info.version ?? "") };
    } catch (error) {
      return { ok: false, text: messages.gtp.failed.replace("{reason}", describe(error)) };
    }
  };

  const openConsole = (id: string | null) => {
    const engine = id ?? consoleEngine ?? gtpEngines[0]?.id ?? null;
    setConsoleEngine(engine);
    setConsoleOpen(true);
    if (engine && backend.gtpConsole) {
      void backend
        .gtpConsole(engine)
        .then((lines) => setConsoleLines((current) => [...current.filter((line) => line.engine_id !== engine), ...lines].slice(-CONSOLE_LIMIT)))
        .catch(report);
    }
  };

  const sendConsole = async (text: string) => {
    if (!consoleEngine || !backend.gtpConsoleSend) return { ok: false, text: messages.gtp.noEngines };
    try {
      return { ok: true, text: await backend.gtpConsoleSend(consoleEngine, text) };
    } catch (error) {
      const refused = typeof error === "object" && error !== null && JSON.stringify(error).includes("StateChangingCommand");
      return { ok: false, text: refused ? messages.gtp.refused.replace("{command}", text.split(/\s+/)[0] ?? text) : describe(error) };
    }
  };

  /** 让AI落子最佳一手 with a line under the pointer plays that line, as far as it shows (Java's playVariation). */
  const bindings = (analysis: AnalysisView | null, hovered: { x: number; y: number } | null): MenuBindings => {
    const idle = !active && !matchActive;
    const ordered = [...(analysis?.candidates ?? [])].sort((a, b) => a.order - b.order);
    const index = hovered ? ordered.findIndex((candidate) => candidate.point?.x === hovered.x && candidate.point?.y === hovered.y) : -1;
    const pick: PvPick | null = index >= 0 && (ordered[index]?.pv.length ?? 0) > 0 ? { candidate: index, moves: ordered[index]!.pv.length } : null;
    const openPlay = { run: () => setDialog("play"), enabled: idle };
    return {
      newGenmoveGame: openPlay,
      newAnalyzeModeGame: openPlay,
      newEngineGame: { run: () => setDialog("match"), enabled: idle },
      continueGenmoveGameAsWhite: { run: () => continueGame("White"), enabled: idle && view.hasDocument },
      continueGenmoveGameAsBlack: { run: () => continueGame("Black"), enabled: idle && view.hasDocument },
      continueAnalyzeGameAsWhite: { run: () => continueGame("White"), enabled: idle && view.hasDocument },
      continueAnalyzeGameAsBlack: { run: () => continueGame("Black"), enabled: idle && view.hasDocument },
      continueGameHere: { run: () => continueGame(null), enabled: idle && view.hasDocument },
      setAiTime: { run: () => setDialog("aiTime"), enabled: active },
      breakGame: { run: () => setConfirm("end"), enabled: active },
      breakEngineGame: { run: () => runMatch(backend.stopMatch), enabled: matchActive },
      pauseEngineGame: { run: () => runMatch(match?.phase === "Paused" ? backend.resumeMatch : backend.pauseMatch), enabled: matchActive, checked: match?.phase === "Paused" },
      changeEngineGameNumbers: { run: () => setDialog("games"), enabled: matchActive },
      intervention: { run: () => setPanelOpen(true), enabled: matchActive },
      gtpPanel: { run: () => (consoleOpen ? setConsoleOpen(false) : openConsole(null)), checked: consoleOpen },
      gtpEngines: { run: () => setDialog("gtp") },
      ...(pick && !active ? { playBestMove: { run: () => void backend.playBestMove?.(pick).catch(report) } } : {}),
    };
  };

  const toolbar: GameToolbar = {
    active,
    paused: game?.phase === "Paused",
    canStart: !active && !matchActive,
    onNew: () => setDialog("play"),
    onPause: () => run(backend.pauseGame),
    onResume: () => run(backend.resumeGame),
    onTakeBack: () => run(backend.takeBack),
    onResign: () => setConfirm("resign"),
    onEnd: () => setConfirm("end"),
    labels: { resume: labels.resume, takeBack: labels.takeBack, resignGame: labels.resignGame, endGame: labels.endGame },
  };

  /** 对弈模式 hides what the engine thinks while the game is on. */
  const hideCandidates = active && focus.focus && !(game?.to_play === "Black" ? focus.showBlack : focus.showWhite);
  const shown = (display: Display): Display => (active && focus.focus ? { ...display, show_winrate_graph: false, show_hover_variation: false, show_list_pane: false } : display);

  const tray = () => {
    const chips = [];
    if (active && game) {
      const human = game.black?.type === "Human" ? "Black" : game.white?.type === "Human" ? "White" : null;
      const clock = human === "Black" ? game.black_clock : human === "White" ? game.white_clock : null;
      const state = game.phase === "Paused" ? labels.paused : game.ai_thinking ? labels.thinking : labels.yourTurn;
      const time = clockText(clock, labels);
      chips.push(
        <span key="game" className="chip play-chip" data-testid="play-chip" data-state={game.phase === "Paused" ? "paused" : game.ai_thinking ? "thinking" : "human"} data-urgent={clockUrgent(clock) || undefined}>
          {game.ai_thinking && game.phase !== "Paused" ? <span className="play-dot" aria-hidden="true" /> : null}
          <span>{state}</span>
          {time ? <span className="play-clock">{`${human === "Black" ? labels.black : labels.white} ${time}`}</span> : null}
        </span>,
      );
    }
    if (match && matchActive && !panelOpen) {
      chips.push(
        <button key="match" type="button" className="chip play-chip" data-testid="match-chip" onClick={() => setPanelOpen(true)}>
          <span>{messages.match.title}</span>
          <span className="play-clock">{`${match.a?.total ?? 0}:${match.b?.total ?? 0} · ${match.completed ?? 0}/${match.games ?? 0}`}</span>
        </button>,
      );
    }
    return chips.length > 0 ? <>{chips}</> : null;
  };

  const close = () => setDialog(null);
  const dialogs = () => (
    <>
      {dialog === "play" ? (
        <PlayDialog saved={lastSetup ?? settings?.game_setup ?? null} gtpEngines={gtpEngines} canContinue={view.hasStones} focus={focus} labels={labels} rules={messages.info.rules} onStart={startGame} onClose={close} />
      ) : null}
      {dialog === "aiTime" && game ? <AiTimeDialog game={game} gtpEngines={gtpEngines} labels={labels} onApply={setAiPlayers} onClose={close} /> : null}
      {dialog === "match" ? (
        <MatchDialog
          katago={katagoEngines}
          gtpEngines={gtpEngines}
          canContinue={view.hasStones}
          labels={messages.match}
          play={labels}
          rules={messages.info.rules}
          pickOpenings={() => backend.pickOpenPaths()}
          onStart={startMatch}
          onClose={close}
        />
      ) : null}
      {dialog === "games" && match ? (
        <GamesDialog
          current={match.games ?? 1}
          done={match.completed ?? 0}
          labels={messages.match}
          onApply={(games) => {
            close();
            runMatch(backend.setMatchGames && (() => backend.setMatchGames!(games)));
          }}
          onClose={close}
        />
      ) : null}
      {dialog === "gtp" ? (
        <GtpEngines
          engines={gtpEngines}
          labels={messages.gtp}
          busy={busy}
          onSave={saveGtp}
          onRemove={(id) => void backend.removeGtpEngine?.(id).then(applySettings).catch(report)}
          onTest={testGtp}
          pickProgram={() => backend.pickEngineFile("program")}
          pickFolder={() => backend.pickDirectory()}
          onClose={close}
        />
      ) : null}
      {panelOpen && match ? (
        <MatchPanel
          view={match}
          names={sides.names}
          katago={sides.katago}
          labels={messages.match}
          controls={{
            pause: () => runMatch(backend.pauseMatch),
            resume: () => runMatch(backend.resumeMatch),
            stop: () => runMatch(backend.stopMatch),
            setGames: () => setDialog("games"),
            playNow: () => runMatch(backend.matchPlayNow),
            resign: (color) => runMatch(backend.matchResign && (() => backend.matchResign!(color))),
            setParams: (side, wrn, pda) => runMatch(backend.setMatchSideParams && (() => backend.setMatchSideParams!(side, wrn, pda))),
            close: () => setPanelOpen(false),
          }}
        />
      ) : null}
      {consoleOpen ? (
        <GtpConsole
          engines={gtpEngines}
          engineId={consoleEngine}
          lines={consoleLines.filter((line) => line.engine_id === consoleEngine)}
          labels={messages.gtp}
          onEngine={openConsole}
          onSend={sendConsole}
          onClear={() => setConsoleLines((lines) => lines.filter((line) => line.engine_id !== consoleEngine))}
          onClose={() => setConsoleOpen(false)}
        />
      ) : null}
      {notice ? <GameNotice view={notice} labels={labels} onClose={() => setNotice(null)} /> : null}
      {confirm ? (
        <ConfirmDialog
          title={confirm === "resign" ? labels.resignGame : labels.endGame}
          lines={[confirm === "resign" ? labels.confirmResign : labels.confirmEnd]}
          confirm={confirm === "resign" ? labels.resignGame : labels.endGame}
          cancel={labels.cancel}
          testId="play-confirm"
          onConfirm={() => {
            const which = confirm;
            setConfirm(null);
            run(which === "resign" ? backend.resignGame : backend.stopGame);
          }}
          onCancel={() => setConfirm(null)}
        />
      ) : null}
      {failure ? <MessageBox title={labels.title} lines={[failure]} ok={labels.close} testId="play-failure" onClose={() => setFailure(null)} /> : null}
    </>
  );

  return {
    game,
    match,
    active,
    toolbar,
    bindings,
    hideCandidates,
    shown,
    tray,
    dialogs,
    /** The window title's lead while an engine match is on: Java's "胜:负 A VS B 胜率 x%". */
    titlePrefix: match && (matchActive || match.phase === "Completed") ? matchTitle(match, sides.names, messages.match) : null,
    dialogOpen: dialog !== null || confirm !== null || failure !== null,
  };
}

