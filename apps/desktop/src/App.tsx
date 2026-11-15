import { useEffect, useMemo, useRef, useState } from "react";
import * as backend from "@/api/backend";
import type { AnalysisLimitSettings, ColorView, EngineStatus, MoveRank, PointView, ProblemMoveSort, ProblemMovesQuery } from "@/api/backend";
import { Board } from "./board/Board";
import type { NextMove } from "./board/draw";
import { SubBoard } from "./board/SubBoard";
import { useVariationPreview } from "./board/useVariationPreview";
import { nextMoveNumbers, nextRankMarks } from "./display/settings";
import { useDisplayView } from "./display/useDisplayView";
import { useDisplay } from "./display/useDisplay";
import { useCommentEdit } from "./edit/useCommentEdit";
import { useBoardMenu } from "./edit/BoardMenu";
import { useBoardModes } from "./edit/useBoardModes";
import { useEditing } from "./edit/useEditing";
import { useCommentary } from "./commentary/useCommentary";
import { useOnline } from "./online/useOnline";
import { remoteSummary } from "./remote/format";
import { useRemote } from "./remote/useRemote";
import { CurveProgress } from "./graph/CurveProgress";
import { liveGraph } from "./graph/scale";
import { WinrateGraph } from "./graph/WinrateGraph";
import { getMessages } from "./locales";
import { AboutDialog } from "./menu/AboutDialog";
import { isEnabled, menuBindings, type MenuBindings } from "./menu/bindings";
import { MenuBar } from "./menu/MenuBar";
import { useShortcuts } from "./menu/shortcuts";
import { useMenus } from "./menu/useMenus";
import { analysisSummary, reviewStatusOf } from "./panels/info";
import { CapturedBox, StatisticsBox } from "./panels/InfoPanel";
import { MoveTree } from "./panels/MoveTree";
import { CandidateList } from "./panels/CandidateList";
import { NotesPanel } from "./panels/NotesPanel";
import { useVariationTree } from "./panels/useVariationTree";
import { AutoAnalysis, type AutoAnalysisHandle } from "./review/AutoAnalysis";
import { ProblemList } from "./review/ProblemList";
import { StrengthDialog } from "./review/StrengthDialog";
import { LimitDialog } from "./analysis/LimitDialog";
import { useAnalysisRun } from "./analysis/useAnalysisRun";
import { useClearing } from "./analysis/useClearing";
import { ToolsPanel, ToolsTabs } from "./engines/EngineTools";
import { useEngines } from "./engines/useEngines";
import { useRelease } from "./release/useRelease";
import { useEveryday } from "./everyday/useEveryday";
import { useBatch } from "./batch/useBatch";
import { useEstimate } from "./estimate/useEstimate";
import { usePlay } from "./play/usePlay";
import { useCoach } from "./coach/useCoach";
import { BOTTOM_ITEMS, itemFilter, TOP_ITEMS } from "./frame/toolbarItems";
import { type ToolsTab, useEngineTools } from "./engines/useEngineTools";
import { BenchmarkChip } from "./benchmark/BenchmarkChip";
import { BenchmarkDialog } from "./benchmark/BenchmarkDialog";
import { useBenchmark } from "./benchmark/useBenchmark";
import { BoardSyncChip } from "./sync/BoardSyncChip";
import { useBoardSync } from "./sync/useBoardSync";
import { TsumegoChip } from "./tsumego/TsumegoChip";
import { TsumegoDialog } from "./tsumego/TsumegoDialog";
import { useTsumego } from "./tsumego/useTsumego";
import { TIER_ORDER } from "./review/strength";
import { useProblemMoves } from "./review/useProblemMoves";
import { useStrength } from "./review/useStrength";
import { useWholeGame } from "./review/useWholeGame";
import { SettingsDialog } from "./settings/SettingsDialog";
import { connect, createStore, type Store, useAppState } from "./state/store";
import { BottomToolbar, JUMP } from "./frame/BottomToolbar";
import { jumpTarget } from "./frame/jump";
import { MainPanel } from "./frame/MainPanel";
import { MessageBox } from "./frame/MessageBox";
import { StatusLine } from "./frame/StatusLine";
import { statusLine } from "./frame/status";
import { ToolButton } from "./frame/ToolButton";
import { TopToolbar } from "./frame/TopToolbar";
import { useOpenFailed } from "./frame/useOpenFailed";
import { useWindowTitle } from "./frame/useWindowTitle";

export { JUMP };

/** Java shows the board synchronization tool on Windows only. */
const WINDOWS = navigator.userAgent.includes("Windows");

/** A wheel notch (or this much trackpad travel) moves one move, as on Java's board. */
const WHEEL_STEP = 40;

function report(error: unknown) {
  console.error("Backend command failed", error);
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}


/** LizzieYzy Next's window: info and graph left, board centre, the game tree right. */
export default function App({ store: provided }: { store?: Store } = {}) {
  const [store] = useState(() => provided ?? createStore());
  useEffect(() => connect(store, backend), [store]);

  const document = useAppState(store, (state) => state.document);
  const analysis = useAppState(store, (state) => state.analysis);
  const engine = useAppState(store, (state) => state.engine);
  const revision = useAppState(store, (state) => state.revision);
  // 设置 → 语言: the saved choice, or the system's language.
  const release = useRelease(backend);
  const messages = getMessages(release.locale);
  const chinese = release.locale.toLowerCase().startsWith("zh");
  const loadRelease = release.load;
  const [pondering, setPondering] = useState(true);
  const [strengthOpen, setStrengthOpen] = useState(false);
  const [strengthBounds, setStrengthBounds] = useState<[number, number] | null>(null);
  const [aboutOpen, setAboutOpen] = useState(false);
  // State, not a ref: menu bindings are built during render and only call it later.
  const [autoAnalysis, setAutoAnalysis] = useState<AutoAnalysisHandle | null>(null);
  const tsumego = useTsumego(backend, revision);
  const setTsumegoOptions = tsumego.setOptions;
  // 显示: saved in the settings, read with them at start.
  const display = useDisplay(backend, messages.display);
  const loadDisplay = display.load;
  // The engines and the main one; a first run without one picks it by itself.
  const engines = useEngines(backend, engine, messages.frame.engineNames, messages.engines, messages.setup, (settings) => display.load(settings.display), {
    labels: messages.remote,
    open: () => remote.open(),
    summary: (entry) => remoteSummary(entry.profile, messages.remote),
  });
  // 远程算力; a remote main engine greys out what only a local one can do.
  const remote = useRemote(backend, messages.remote, engines);
  const localOnly = <T extends MenuBindings>(entries: T): MenuBindings => (remote.remote ? Object.fromEntries(Object.keys(entries).map((id) => [id, { enabled: false }])) : entries);
  const loadEngines = engines.load;
  // 人机对局, 引擎对局 and GTP engines; 对弈模式 hides what the engine thinks during a game.
  const games = usePlay(backend, engines.settings, engines.apply, { hasDocument: document !== null, hasStones: document?.board?.stones.some(Boolean) ?? false }, messages);
  // AI 陪练; pass and jump are defined below and only called later, from the panel.
  const coach = useCoach(backend, messages, { hasStones: document?.board?.stones.some(Boolean) ?? false, board: document?.board ?? null, tree: document?.tree ?? null, pass: () => pass(), jump: (node) => jump(node) });
  const shown = coach.shown(games.shown(display.display));
  // Kata形势, 纯网络 and 策略网络 on both boards.
  const estimate = useEstimate(backend, shown, display.set, analysis);
  const everyday = useEveryday(
    backend,
    { board: document?.board ?? null, fileName: document?.file_name ?? null, subBoard: shown.show_sub_board, graph: shown.show_winrate_graph },
    { value: engines.options.read_komi, set: (read_komi) => void engines.saveOptions({ ...engines.options, read_komi }).catch(report) },
  );
  const loadEveryday = everyday.load;
  const wheel = useRef(0);
  // 野狐棋谱, 腾讯棋谱 and 打开在线链接; AI 解说.
  const online = useOnline(backend, messages);
  const loadOnline = online.load;
  const commentary = useCommentary(backend, messages.commentary, { board: document?.board ?? null, jump: (node) => jump(node) });
  const loadCommentary = commentary.load;
  const [boardHover, setBoardHover] = useState<PointView | null>(null);
  // A row of the candidate list under the pointer previews like a hovered candidate.
  const [listHover, setListHover] = useState<PointView | null>(null);
  const { tree, live: liveTree } = useVariationTree(backend, document);
  const plainWholeGame = useWholeGame(backend);
  // 批量分析 and 精析; while a batch runs, the whole-game controls steer it.
  const batch = useBatch(backend, plainWholeGame, { hasDocument: document !== null, modified: document?.modified ?? false }, messages.batch);
  const wholeGame = batch.controls;
  const loadBatch = batch.load;
  // LizzieYzy Next draws the graph with a one-visit pass when a game opens; that pass stays quiet.
  const [autoCurve, setAutoCurve] = useState(true);
  const [cacheOn, setCacheOn] = useState(true);
  const quickPass = wholeGame.progress?.source === "AutomaticQuick" ? wholeGame.progress : null;
  const quickRunning = quickPass !== null && (quickPass.status.type === "Running" || quickPass.status.type === "Paused");
  const sync = useBoardSync(backend);
  const tuning = useBenchmark(backend);
  // 引擎一键设置, by the tab shown; null while closed.
  const [toolsTab, setToolsTab] = useState<ToolsTab | null>(null);
  const tuningOpen = toolsTab !== null;
  const tools = useEngineTools(backend, tuningOpen, engines.apply);
  // Live analysis as it runs now: paused by the user, or stopped at a time or visit limit.
  const run = useAnalysisRun(backend);
  const [limits, setLimits] = useState<AnalysisLimitSettings | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const limitStop = run.state?.pause_reason === "TimeLimit" || run.state?.pause_reason === "PlayoutLimit";
  // Java tells once per engine session that a move reached its limit, in a box that stays until closed.
  const [limitTips, setLimitTips] = useState<"unseen" | "open" | "done">("unseen");
  if (limitStop && limitTips === "unseen") setLimitTips("open");
  // Java's defaults: keep the board in step with the client, but leave the viewed move alone.
  const [syncOptions, setSyncOptions] = useState({ always_sync: true, always_goto_last: false });
  const [problemColor, setProblemColor] = useState<ColorView | null>(null);
  const [problemSort, setProblemSort] = useState<ProblemMoveSort>("LossDesc");
  const [notesTab, setNotesTab] = useState<"comment" | "problems">("comment");
  // A game that cannot be opened, picked or dropped, is reported by Rust; say so as Java does.
  const [openFailure, closeOpenFailure] = useOpenFailed(backend);
  const problemQuery = useMemo<ProblemMovesQuery>(
    () => ({ minimum_rank: "Inaccuracy", color: problemColor, sort: problemSort }),
    [problemColor, problemSort],
  );
  const reviewSignals = { revision, progress: wholeGame.progress?.completed ?? 0, live: analysis?.visits ?? 0 };
  const problems = useProblemMoves(backend, problemQuery, reviewSignals);
  const strength = useStrength(backend, reviewSignals, strengthBounds);
  // Match rates join the player rows only once the estimate has enough moves to mean something.
  const strengthReport = strength?.report.has_enough_data ? strength.report : null;

  useEffect(() => {
    let active = true;
    backend
      .settings()
      .then(async (settings) => {
        if (!active) return;
        setPondering(settings.pondering);
        setAutoCurve(settings.auto_quick_analyze_on_load ?? true);
        setCacheOn(settings.cache_enabled ?? true);
        loadOnline(settings);
        loadCommentary(settings);
        setTsumegoOptions(settings.tsumego);
        loadDisplay(settings.display);
        loadRelease(settings);
        loadEveryday(settings);
        loadBatch(settings);
        await loadEngines(settings, () => active);
      })
      .catch(report);
    return () => {
      active = false;
    };
    // A state setter: the settings are read once.
  }, [setTsumegoOptions, loadDisplay, loadEngines, loadRelease, loadEveryday, loadBatch, loadOnline, loadCommentary]);

  // A comment being edited is saved before the app moves to another move by itself.
  const comment = useCommentEdit(backend, document);
  const step = (steps: number) => {
    comment.commit();
    backend.step(steps).catch(report);
  };
  const board = document?.board ?? null;
  const points = useMemo(() => (document ? liveGraph(document.graph, analysis) : []), [document, analysis]);
  const line = useMemo(() => points.map((point) => point.node_id), [points]);
  const currentIndex = document?.graph.current_index ?? 0;
  const currentNode = document?.tree.current ?? null;
  // The sub-board plays out the best move's line, or the hovered candidate's; the main board
  // shows the same replay only while a candidate is hovered.
  const bestPoint = useMemo(() => {
    if (!analysis || analysis.node_id !== currentNode) return null;
    const best = analysis.candidates.reduce<(typeof analysis.candidates)[number] | null>(
      (first, candidate) => (candidate.point && candidate.pv.length > 0 && (!first || candidate.order < first.order) ? candidate : first),
      null,
    );
    return best?.point ?? null;
  }, [analysis, currentNode]);
  const hovered = boardHover ?? listHover;
  const replay = useVariationPreview(backend, currentNode, revision, analysis, hovered ?? bestPoint);
  const variation = hovered && shown.show_hover_variation ? replay : null;
  const info = document?.game_info ?? null;
  // The mistake bars share the graph's line and indexing; anything else is stale.
  const mistakes = problems && problems.error_bar.length === points.length ? problems.error_bar : undefined;
  const analysedCount = useMemo(() => points.filter((point) => point.summary !== null).length, [points]);
  const analysed = analysedCount > 0;
  const ranks = useMemo(() => {
    const byNode = new Map<number, MoveRank>();
    for (const point of mistakes ?? []) if (point.loss) byNode.set(point.node_id, point.loss.rank);
    return byNode;
  }, [mistakes]);
  // The mark on the last move: its rank, known once it and the move before it are analysed.
  const lastRank = useMemo(
    () => mistakes?.find((point) => point.node_id === currentNode)?.loss?.rank ?? null,
    [mistakes, currentNode],
  );
  const currentTreeNode = useMemo(() => liveTree?.nodes.find((node) => node.id === currentNode) ?? null, [liveTree, currentNode]);
  // The recorded continuations, each with the position it leads to when that is analysed.
  const nextMoves = useMemo<NextMove[]>(() => {
    if (!liveTree || !currentTreeNode) return [];
    const moves: NextMove[] = [];
    for (const id of currentTreeNode.children) {
      const mv = liveTree.nodes.find((node) => node.id === id)?.mv;
      if (mv?.type !== "Play") continue;
      moves.push({ point: mv.data.point, after: points.find((point) => point.node_id === id)?.summary ?? null });
    }
    return moves;
  }, [liveTree, currentTreeNode, points]);
  const { marks, style, nextHint, hidden, scoreOffset, scorePoints, names: boardNames } = useDisplayView(
    shown,
    info,
    document?.tree.branch_start_move_number ?? null,
    ranks,
    points,
  );
  const emptyGame =
    board !== null &&
    document?.tree.current === document?.tree.root &&
    currentTreeNode !== null &&
    currentTreeNode.children.length === 0 &&
    board.stones.every((stone) => stone === null);
  const play = (point: PointView) => {
    if (!board) return;
    backend.play({ type: "Play", data: { color: board.to_play, point } }).catch(report);
  };
  const openFile = async () => {
    const path = await backend.pickOpenPath();
    if (path) await backend.openPath(path);
  };
  const clearing = useClearing(backend, board !== null, messages.clearing);
  const modes = useBoardModes(backend, document, messages.modes);
  const editing = useEditing(
    backend,
    document,
    emptyGame,
    messages.edit,
    messages.info.rules,
    openFailure,
    { newBoard: shown.confirm_new_board, replace: shown.confirm_overwrite },
    (kind) => display.set(kind === "newBoard" ? { confirm_new_board: false } : { confirm_overwrite: false }),
  );
  const boardMenu = useBoardMenu(backend, document, analysis, revision, messages.boardMenu, modes.startMove);
  // Space and the chip resume a limit stop without changing the saved setting.
  const togglePondering = run.toggle;
  const livePondering = run.state?.pondering ?? pondering;
  useEffect(() => {
    backend.analysisLimits().then(setLimits).catch(report);
  }, []);
  const jump = (nodeId: number) => {
    comment.commit();
    backend.goTo(nodeId).catch(report);
  };
  const pass = () => {
    if (board) backend.play({ type: "Pass", data: { color: board.to_play } }).catch(report);
  };
  const current = document?.tree.current ?? null;
  const best = analysis && analysis.node_id === current ? analysis.candidates[0] : undefined;
  const wholeGameStatus = wholeGame.progress?.status.type;
  const hawkEyeOpen = shown.show_comment && notesTab === "problems";
  const toggleHawkEye = () => {
    if (hawkEyeOpen) return setNotesTab("comment");
    setNotesTab("problems");
    if (!shown.show_comment) display.set({ show_comment: true });
  };
  const bindings: MenuBindings = batch.guard({
    ...menuBindings(
      {
        hasDocument: document !== null,
        hasBoard: board !== null,
        wholeGameActive: wholeGameStatus === "Running" || wholeGameStatus === "Paused",
        hasBestMove: board !== null && best !== undefined,
        engineConfigured: engines.profile !== null,
        windows: WINDOWS,
        syncPhase: sync.status?.phase ?? "Stopped",
        syncOptions,
        tree: liveTree,
        current,
        canUndo: document?.can_undo ?? false,
        canRedo: document?.can_redo ?? false,
        recent: editing.recent,
        setupTool: modes.mode.kind === "setup" ? modes.mode.tool : null,
        setupEditable: modes.setupEditable,
        trial: editing.trial,
        playColor: modes.mode.kind === "color" ? modes.mode.color : null,
        insertColor: modes.mode.kind === "insert" ? modes.mode.color : null,
      },
      {
        open: () => void openFile().catch(report),
        openRecent: editing.openRecent,
        clearRecent: editing.clearRecent,
        newBoard: editing.newBoard,
        setBoardSize: editing.openBoardSize,
        editGameInfo: editing.openGameInfo,
        save: editing.save,
        saveAs: editing.saveAs,
        deleteMove: editing.deleteMove,
        deleteBranch: editing.deleteBranch,
        setAsMain: editing.setAsMain,
        undo: editing.undo,
        redo: editing.redo,
        toggleSetup: modes.toggleSetup,
        setupTool: modes.setupTool,
        clearSetup: modes.clearSetup,
        setFirstPlayer: modes.setFirstPlayer,
        convertToStart: modes.convertToStart,
        colorMode: modes.colorMode,
        alternate: modes.exit,
        insertMode: modes.insertMode,
        quit: () => void backend.quit().catch(report),
        togglePondering,
        openAutoAnalysis: () => autoAnalysis?.show(),
        flashAnalyze: () => wholeGame.start(1),
        cancelAutoAnalysis: wholeGame.cancel,
        openStrength: () => setStrengthOpen(true),
        playBestMove: () => {
          if (best?.point) play(best.point);
          else if (best) pass();
        },
        pass,
        step,
        goTo: jump,
        openFox: online.openFox,
        openTencent: online.openTencent,
        openUrl: online.openUrl,
        startBoardSync: () => sync.start({ ...syncOptions, language: chinese ? "Cn" : "En" }),
        toggleSyncOption: (key) => setSyncOptions((options) => ({ ...options, [key]: !options[key] })),
        openTsumego: () => tsumego.setOpen(true),
        openBenchmark: () => setToolsTab("overview"),
        openSettings: () => setSettingsOpen(true),
        openAbout: () => setAboutOpen(true),
      },
    ),
    ...display.bindings,
    ...engines.bindings,
    ...clearing.bindings,
    ...release.bindings,
    ...everyday.bindings,
    ...batch.bindings,
    ...estimate.bindings,
    ...games.bindings(analysis, hovered),
    ...localOnly(coach.bindings),
    ...commentary.bindings,
    ...remote.bindings,
    ...(remote.remote ? { autoSetup: { enabled: false } } : {}),
    // 超级鹰眼: Java's problem-move window is the 问题手 page here.
    hawkEye: { run: toggleHawkEye, enabled: document !== null, checked: hawkEyeOpen },
    hawkEye2: { run: toggleHawkEye, enabled: document !== null },
  });
  useShortcuts(
    bindings,
    !(engines.dialogOpen || strengthOpen || online.dialogOpen || commentary.dialogOpen || remote.dialogOpen || aboutOpen || tsumego.open || tuningOpen || settingsOpen || editing.dialogOpen || modes.dialogOpen || display.dialogOpen || clearing.dialogOpen || release.dialogOpen || everyday.dialogOpen || batch.dialogOpen || games.dialogOpen || coach.dialogOpen),
  );
  const frame = messages.frame;
  const engineLabel = remote.label(engines.label === "" ? null : engines.label);
  // "No engine" is for no engine: one that Rust has not named yet shows without a name.
  const menus = useMenus(WINDOWS, editing.recent, messages.edit, remote.label(engines.label), shown.last_move_rank_marks, shown.next_move_min_visits, { names: engines.menuNames, main: engines.mainIndex }, messages.engines.importMenu, messages.gtp.menu);
  const autoAnalyzing = quickRunning || wholeGameStatus === "Running";
  useWindowTitle(backend, {
    app: messages.app.title,
    analysis,
    current,
    engine: engineLabel,
    pondering: livePondering,
    info,
    file: document?.file_name ?? null,
    trial: editing.trial,
    labels: frame.title,
    match: games.titlePrefix,
  });
  const reviewing = wholeGameStatus === "Running" || wholeGameStatus === "Paused";
  const reviewStatus = reviewStatusOf(reviewing ? wholeGame.progress : null, analysedCount, points.length, messages.notes);
  const summaryLines = board ? analysisSummary(points, currentIndex, board.last_move, engineLabel, info?.komi ?? 0, messages.notes) : [];
  // First-run setup reads as starting.
  const shownEngine: EngineStatus = engines.autoSetup && engine.type === "Stopped" ? { type: "Starting" } : engine;
  const status = statusLine({
    engine: shownEngine,
    name: engineLabel,
    configured: engines.profile !== null,
    autoAnalyzing,
    pondering: livePondering,
    saved: analysis?.source === "Saved" && analysis.node_id === current,
    labels: frame.status,
    states: messages.engine,
  });

  // 合并在菜单栏上 puts the toolbar in the menu bar's row; otherwise it has its own.
  const combinedTop = shown.top_toolbar_layout === "Combined";
  const topToolbar = (
<TopToolbar
      labels={frame.top}
      hasDocument={document !== null}
      hasBoard={board !== null}
      offMainLine={isEnabled(bindings.backToMainBranch)}
      komi={info?.komi ?? null}
      onNew={editing.newBoard}
      onSetMain={editing.setAsMain}
      onKomi={editing.setKomi}
      onBoardSize={editing.openBoardSize}
      markupOpen={modes.mode.kind === "markup"}
      onMarkup={modes.toggleMarkup}
      hawkEye={hawkEyeOpen}
      onHawkEye={toggleHawkEye}
      rankMarks={shown.move_rank_marks !== "None"}
      onRankMarks={() => display.set(nextRankMarks(shown))}
      onChangeTurn={modes.toggleToPlay}
      trial={editing.trial}
      playColor={modes.mode.kind === "color" ? modes.mode.color : null}
      onColor={modes.colorMode}
      onAlternate={modes.exit}
      onOpen={() => void openFile().catch(report)}
      onSave={editing.save}
      onFox={online.openFox}
      onTencent={online.openTencent}
      onStrength={() => setStrengthOpen(true)}
      onAutoSetup={() => setToolsTab("overview")}
      autoSetupBlocked={remote.remote ? messages.remote.unsupported : undefined}
      onFlash={() => batch.bindings.flashAnalyzeAllGame?.run?.()}
      onBackMain={() => bindings.backToMainBranch?.run?.()}
      onPass={pass}
      onTsumego={() => tsumego.setOpen(true)}
      show={itemFilter(TOP_ITEMS, shown.top_toolbar_hidden)}
      combined={combinedTop}
      wrap={shown.top_toolbar_wrap}
      onSwitchLayout={() => display.set({ top_toolbar_layout: combinedTop ? "Separate" : "Combined" })}
      limits={limits}
      onLimits={(next) => void backend.setAnalysisLimits(next).then(setLimits).catch(report)}
      game={games.toolbar}
    />
  );

  return (
    <main className="app">
      <h1 className="visually-hidden">{messages.app.title}</h1>
      <MenuBar
        menus={menus}
        bindings={bindings}
        labels={messages.menu}
        label={messages.menuBar.label}
        actions={
          <>
            {shown.show_top_toolbar && combinedTop ? topToolbar : null}
            {remote.remote ? <ToolButton label={frame.menu.aiCoach} title={messages.remote.unsupported} enabled={false} /> : coach.button(frame.menu)}
            {commentary.button(frame.menu)}
          </>
        }
        tray={
          <>
            {release.tray(messages.release)}
            {batch.tray()}
            {games.tray()}
            {editing.trialChip}
            {modes.chip}
            {tsumego.status ? <TsumegoChip status={tsumego.status} labels={messages.tsumego} onExit={tsumego.exit} /> : null}
            {sync.status && sync.status.phase !== "Stopped" ? <BoardSyncChip status={sync.status} labels={messages.sync} onStop={sync.stop} /> : null}
            {tuning.status && !tuningOpen ? (
              <BenchmarkChip status={tuning.status} labels={messages.benchmark} onOpen={() => setToolsTab("benchmark")} onStop={tuning.cancel} />
            ) : null}
          </>
        }
      />
      {shown.show_top_toolbar && !combinedTop ? topToolbar : null}
      <MainPanel
        hidden={hidden}
        boardSize={board?.width ?? 19}
        boardData={{ "data-revision": revision, "data-node-id": document?.tree.current }}
        onBoardWheel={(event) => {
          // Lines (Firefox, some mice) count as whole notches.
          wheel.current += event.deltaMode === 0 ? event.deltaY : Math.sign(event.deltaY) * WHEEL_STEP;
          if (Math.abs(wheel.current) < WHEEL_STEP) return;
          const steps = Math.sign(wheel.current);
          wheel.current = 0;
          step(steps);
        }}
        regions={{
          captured: (
            <CapturedBox
              board={board}
              game={info}
              blackMatch={strengthReport && strengthReport.black.sample_count > 0 ? strengthReport.black.match_rate : null}
              whiteMatch={strengthReport && strengthReport.white.sample_count > 0 ? strengthReport.white.match_rate : null}
              labels={messages.info}
            />
          ),
          statistics: (
            <div
              className="statistics"
              data-testid="analysis-move"
              data-node-id={analysis?.node_id}
              data-revision={analysis?.revision}
              data-visits={analysis?.visits}
              title={analysis ? messages.board.visits.replace("{visits}", String(analysis.visits)) : undefined}
            >
              <StatisticsBox board={board} points={scorePoints} currentIndex={currentIndex} labels={messages.info} />
            </div>
          ),
          graph: (
            <div className="graph-region">
              {quickRunning && quickPass ? <CurveProgress done={quickPass.completed} total={quickPass.total} labels={messages.graph} onStop={wholeGame.cancel} /> : null}
              {document ? (
                <WinrateGraph
                  graph={document.graph}
                  analysis={analysis}
                  mistakes={mistakes}
                  mode={shown.winrate_graph_mode}
                  fill={shown.fill_winrate_graph}
                  bars={shown.show_blunder_bar}
                  scoreOffset={scoreOffset}
                  labels={messages.graph}
                  onJump={jump}
                />
              ) : null}
            </div>
          ),
          comment: (
            <NotesPanel
              tab={notesTab}
              onTab={setNotesTab}
              problemCount={problems?.entries.length ?? 0}
              status={reviewStatus}
              summary={summaryLines}
              comment={document?.comment ?? null}
              editor={comment.text === null ? null : { text: comment.text, onChange: comment.change, onCommit: comment.commit, onCancel: comment.cancel }}
              onEdit={document ? comment.begin : undefined}
              labels={messages.notes}
            >
              <ProblemList
                view={problems}
                current={document?.tree.current ?? -1}
                color={problemColor}
                sort={problemSort}
                analysed={analysed}
                labels={messages.problems}
                onColor={setProblemColor}
                onSort={setProblemSort}
                onJump={jump}
              />
            </NotesPanel>
          ),
          status: (
            <span data-testid="engine-status" data-state={shownEngine.type} title={status.detail}>
              <StatusLine text={status.text} />
            </span>
          ),
          board: board ? (
            <Board
              board={board}
              analysis={analysis}
              coordinates={shown.show_coordinates}
              {...estimate.board}
              maxCandidates={games.hideCandidates || coach.hidesAnalysis ? 0 : shown.max_suggestions > 0 ? shown.max_suggestions : board.width * board.height}
              label={messages.board.label}
              preview={listHover}
              variation={variation}
              names={boardNames}
              lastRank={lastRank}
              next={nextMoves}
              nextHint={nextHint}
              marks={marks}
              style={style}
              passLabel={messages.board.pass}
              emptyHint={emptyGame ? messages.board.emptyHint : null}
              markup={coach.markup(document?.markup ?? undefined)}
              editing={modes.editing}
              onPoint={modes.onPoint}
              onContextMenu={(point, x, y) => {
                if (!modes.secondary(point)) boardMenu.open(point, x, y);
              }}
              onHoverCandidate={setBoardHover}
              onPlay={play}
            />
          ) : (
            <p className="board-message" data-testid="position-error">
              {document?.position_error ? messages.board.positionError.replace("{reason}", document.position_error) : messages.board.loading}
            </p>
          ),
          list: coach.panel() ?? (
            <CandidateList
              analysis={analysis && analysis.node_id === current ? analysis : null}
              toPlay={board?.to_play ?? "Black"}
              blackWinrate={shown.winrate_always_black}
              scoreOffset={scoreOffset}
              labels={messages.candidates}
              onHover={setListHover}
              onPlay={play}
            />
          ),
          tree: (
            <MoveTree tree={tree} current={document?.tree.current ?? -1} line={line} boardSize={board?.width ?? 19} ranks={ranks} label={messages.tree.label} onJump={jump} />
          ),
          subBoard: board ? <SubBoard board={board} node={currentNode} revision={revision} variation={replay} label={messages.tree.subBoard} {...estimate.subBoard} /> : null,
        }}
      />
      {shown.show_bottom_toolbar ? (
        <BottomToolbar
          labels={frame.bottom}
          pondering={livePondering}
          hasBoard={board !== null}
          ownership={estimate.on}
          coordinates={shown.show_coordinates}
          show={itemFilter(BOTTOM_ITEMS, shown.bottom_toolbar_hidden)}
          pureNetwork={estimate.pure}
          onPureNetwork={estimate.bindings.showHeatmap?.run}
          onOpen={() => void openFile().catch(report)}
          onSave={editing.save}
          onHawkEye={toggleHawkEye}
          onList={() => display.set({ show_list_pane: !shown.show_list_pane })}
          onRefresh={bindings.clearThisLizzieCache?.run}
          onBackMain={() => bindings.backToMainBranch?.run?.()}
          canBackMain={isEnabled(bindings.backToMainBranch)}
          onMoveRank={() => display.set(nextRankMarks(shown))}
          rankMarks={shown.move_rank_marks !== "None"}
          moveNumbers={shown.move_numbers !== "None"}
          canSetMain={isEnabled(bindings.setAsMain)}
          canDelete={isEnabled(bindings.delete)}
          autoAnalysis={
            <AutoAnalysis
              ref={setAutoAnalysis}
              placement="above"
              progress={quickPass ? null : wholeGame.progress}
              positions={points.length}
              labels={messages.review}
              onStart={wholeGame.start}
              onPause={wholeGame.pause}
              onResume={wholeGame.resume}
              onCancel={wholeGame.cancel}
              autoCurve={autoCurve}
              onAutoCurve={(enabled) => {
                backend
                  .setAutoQuickAnalyzeOnLoad(enabled)
                  .then(setAutoCurve)
                  .catch(report);
              }}
            />
          }
          onTogglePondering={togglePondering}
          onToggleOwnership={estimate.toggle}
          onToggleCoordinates={() => display.set({ show_coordinates: !shown.show_coordinates })}
          onToggleMoveNumbers={() => display.set(nextMoveNumbers(shown))}
          onGotoMove={(n) => {
            const target = jumpTarget(points, document?.tree.main_line ?? [], n);
            if (target !== null && target !== current) jump(target);
          }}
          onStep={step}
          onSetMain={editing.setAsMain}
          onClear={editing.newBoard}
          onDelete={editing.deleteMove}
          trial={editing.trial}
          onTrial={editing.toggleTrial}
          onRemote={() => remote.open()}
        />
      ) : null}
      {editing.dialogs}
      {display.dialog}
      {clearing.dialog}
      {release.dialogs(messages.release)}
      {everyday.dialogs(messages.everyday, messages.display)}
      {batch.dialogs(messages.review)}
      {games.dialogs()}
      {coach.dialogs()}
      {modes.dialog}
      {boardMenu.menu}
      {limitTips === "open" ? <LimitDialog labels={messages.limit} onClose={() => setLimitTips("done")} /> : null}
      {openFailure ? (
        <MessageBox
          title={frame.openFailed.title}
          lines={[fileName(openFailure.path), openFailure.reason]}
          ok={frame.openFailed.ok}
          testId="open-failed"
          onClose={closeOpenFailure}
        />
      ) : null}
      {strengthOpen ? (
        <StrengthDialog
          view={strength}
          progress={wholeGame.progress}
          black={{ name: info?.black_name || messages.info.black, rank: info?.black_rank ?? null }}
          white={{ name: info?.white_name || messages.info.white, rank: info?.white_rank ?? null }}
          chinese={chinese}
          labels={messages.strength}
          tierNames={TIER_ORDER.map((rank) => messages.problems.ranks[rank])}
          bounds={strengthBounds}
          onBounds={setStrengthBounds}
          onAnalyze={() => wholeGame.start(1000)}
          onClose={() => setStrengthOpen(false)}
        />
      ) : null}
      {online.dialogs}
      {commentary.windows}
      {remote.dialog}
      {aboutOpen ? <AboutDialog name={messages.app.title} labels={messages.about} onClose={() => setAboutOpen(false)} /> : null}
      {settingsOpen && limits ? (
        <SettingsDialog
          values={{ limits, autoCurve, cache: cacheOn, engine: engines.options, autoCheckUpdates: release.autoCheck }}
          labels={messages.prefs}
          onSave={async (values) => {
            setLimits(await backend.setAnalysisLimits(values.limits));
            if (values.autoCurve !== autoCurve) setAutoCurve(await backend.setAutoQuickAnalyzeOnLoad(values.autoCurve));
            if (values.cache !== cacheOn) setCacheOn(await backend.setCacheEnabled(values.cache));
            if (values.engine && JSON.stringify(values.engine) !== JSON.stringify(engines.options)) await engines.saveOptions(values.engine);
            if (values.autoCheckUpdates !== undefined && values.autoCheckUpdates !== release.autoCheck) release.chooseAutoCheck(values.autoCheckUpdates);
            setSettingsOpen(false);
          }}
          onClose={() => setSettingsOpen(false)}
        />
      ) : null}
      {toolsTab ? (
        <BenchmarkDialog
          title={messages.tools.title}
          tabs={<ToolsTabs tab={toolsTab} labels={messages.tools} onTab={setToolsTab} />}
          panel={toolsTab === "benchmark" ? undefined : <ToolsPanel tab={toolsTab} tools={tools} labels={messages.tools} currentModel={engines.profile?.model ?? null} />}
          status={tuning.status}
          model={engineLabel}
          labels={messages.benchmark}
          onStart={tuning.start}
          onStop={tuning.cancel}
          onReset={tuning.reset}
          onClose={() => setToolsTab(null)}
        />
      ) : null}
      {tsumego.open ? (
        <TsumegoDialog options={tsumego.options} labels={messages.tsumego} onConfirm={tsumego.generate} onClose={() => tsumego.setOpen(false)} />
      ) : null}
      {engines.dialogs}
    </main>
  );
}
