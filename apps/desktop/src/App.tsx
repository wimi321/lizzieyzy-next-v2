import { useEffect, useMemo, useRef, useState } from "react";
import * as backend from "@/api/backend";
import type { AnalysisLimitSettings, AutoSetupError, ColorView, EngineProfile, EngineStatus, FoxAccount, MoveRank, OpenFailed, PointView, ProblemMoveSort, ProblemMovesQuery, TsumegoOptions, TsumegoStatus, VariationTreeView } from "@/api/backend";
import { Board } from "./board/Board";
import type { NextMove } from "./board/draw";
import { SubBoard } from "./board/SubBoard";
import { useVariationPreview } from "./board/useVariationPreview";
import { FoxDialog } from "./fox/FoxDialog";
import { CurveProgress } from "./graph/CurveProgress";
import { liveGraph } from "./graph/scale";
import { WinrateGraph } from "./graph/WinrateGraph";
import { getMessages } from "./locales";
import { AboutDialog } from "./menu/AboutDialog";
import { isEnabled, menuBindings } from "./menu/bindings";
import { MenuBar } from "./menu/MenuBar";
import { useShortcuts } from "./menu/shortcuts";
import { engineMenu, menusFor } from "./menu/spec";
import { analysisSummary } from "./panels/info";
import { CapturedBox, StatisticsBox } from "./panels/InfoPanel";
import { MoveTree } from "./panels/MoveTree";
import { NotesPanel, type ReviewStatus } from "./panels/NotesPanel";
import { AutoAnalysis, type AutoAnalysisHandle } from "./review/AutoAnalysis";
import { ProblemList } from "./review/ProblemList";
import { StrengthDialog } from "./review/StrengthDialog";
import { LimitDialog } from "./analysis/LimitDialog";
import { useAnalysisRun } from "./analysis/useAnalysisRun";
import { BenchmarkChip } from "./benchmark/BenchmarkChip";
import { BenchmarkDialog } from "./benchmark/BenchmarkDialog";
import { useBenchmark } from "./benchmark/useBenchmark";
import { BoardSyncChip } from "./sync/BoardSyncChip";
import { useBoardSync } from "./sync/useBoardSync";
import { TsumegoChip } from "./tsumego/TsumegoChip";
import { TsumegoDialog } from "./tsumego/TsumegoDialog";
import { TIER_ORDER } from "./review/strength";
import { useProblemMoves } from "./review/useProblemMoves";
import { useStrength } from "./review/useStrength";
import { useWholeGame } from "./review/useWholeGame";
import { EngineSetup } from "./settings/EngineSetup";
import { SettingsDialog } from "./settings/SettingsDialog";
import { connect, createStore, type Store, useAppState } from "./state/store";
import { BottomToolbar, JUMP } from "./frame/BottomToolbar";
import { MainPanel } from "./frame/MainPanel";
import { MessageBox } from "./frame/MessageBox";
import { StatusLine } from "./frame/StatusLine";
import { evaluationText, joinParts, windowTitle } from "./frame/title";
import { ToolButton } from "./frame/ToolButton";
import { TopToolbar } from "./frame/TopToolbar";
import { useEngineName } from "./frame/useEngineName";

export { JUMP };

/** Candidates drawn on the board. */
const MAX_CANDIDATES = 10;

/** Java shows the board synchronization tool on Windows only. */
const WINDOWS = navigator.userAgent.includes("Windows");

/** Java's starting 死活题 options, until the settings say otherwise. */
const TSUMEGO_DEFAULTS: TsumegoOptions = { to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1 };

/** A wheel notch (or this much trackpad travel) moves one move, as on Java's board. */
const WHEEL_STEP = 40;

function report(error: unknown) {
  console.error("Backend command failed", error);
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Why automatic setup gave up, for the setup dialog it opens instead. */
function autoSetupFailure(error: unknown, labels: { autoNoPrograms: string; autoNoModels: string; autoAllFailed: string }): string {
  const failure =
    typeof error === "object" && error !== null && "type" in error && error.type === "AutoSetup" && "data" in error
      ? (error.data as AutoSetupError)
      : null;
  if (failure?.type === "NoPrograms") return labels.autoNoPrograms;
  if (failure?.type === "NoModels") return labels.autoNoModels;
  if (failure?.type === "AllFailed") {
    const tried = failure.data.attempts.map((attempt) => `${fileName(attempt.program)} + ${fileName(attempt.model)}: ${attempt.reason}`);
    return [labels.autoAllFailed.replace("{n}", String(failure.data.attempts.length)), ...tried, failure.data.reason].join("\n");
  }
  if (typeof error === "object" && error !== null && "data" in error && typeof error.data === "string") return error.data;
  return error instanceof Error ? error.message : String(error);
}

/** LizzieYzy Next's window: info and graph left, board centre, the game tree right. */
export default function App({ store: provided }: { store?: Store } = {}) {
  const [store] = useState(() => provided ?? createStore());
  useEffect(() => connect(store, backend), [store]);

  const document = useAppState(store, (state) => state.document);
  const analysis = useAppState(store, (state) => state.analysis);
  const engine = useAppState(store, (state) => state.engine);
  const revision = useAppState(store, (state) => state.revision);
  const messages = getMessages(navigator.language);
  const chinese = navigator.language.toLowerCase().startsWith("zh");
  const [coordinates, setCoordinates] = useState(true);
  const [ownership, setOwnership] = useState(false);
  const [pondering, setPondering] = useState(true);
  const [profile, setProfile] = useState<EngineProfile | null>(null);
  const [setupOpen, setSetupOpen] = useState(false);
  // First run picks an engine by itself; setup opens only when that fails, saying why.
  const [autoSetup, setAutoSetup] = useState(false);
  const [autoSetupFailed, setAutoSetupFailed] = useState<{ error: unknown } | null>(null);
  const [foxOpen, setFoxOpen] = useState(false);
  // Mounted on first use and then only hidden, so a minimized window keeps its list.
  const [foxUsed, setFoxUsed] = useState(false);
  const [strengthOpen, setStrengthOpen] = useState(false);
  const [strengthBounds, setStrengthBounds] = useState<[number, number] | null>(null);
  const [aboutOpen, setAboutOpen] = useState(false);
  // State, not a ref: menu bindings are built during render and only call it later.
  const [autoAnalysis, setAutoAnalysis] = useState<AutoAnalysisHandle | null>(null);
  const [tsumegoOpen, setTsumegoOpen] = useState(false);
  const [tsumegoOptions, setTsumegoOptions] = useState<TsumegoOptions>(TSUMEGO_DEFAULTS);
  // Set while a framed problem is on the board; null otherwise.
  const [tsumego, setTsumego] = useState<TsumegoStatus | null>(null);
  const wheel = useRef(0);
  const [foxHistory, setFoxHistory] = useState<FoxAccount[]>([]);
  const [boardHover, setBoardHover] = useState<PointView | null>(null);
  const [tree, setTree] = useState<VariationTreeView | null>(null);
  const wholeGame = useWholeGame(backend);
  // LizzieYzy Next draws the graph with a one-visit pass when a game opens; that pass stays quiet.
  const [autoCurve, setAutoCurve] = useState(true);
  const quickPass = wholeGame.progress?.source === "AutomaticQuick" ? wholeGame.progress : null;
  const quickRunning = quickPass !== null && (quickPass.status.type === "Running" || quickPass.status.type === "Paused");
  const sync = useBoardSync(backend);
  const tuning = useBenchmark(backend);
  const [tuningOpen, setTuningOpen] = useState(false);
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
  const [openFailure, setOpenFailure] = useState<OpenFailed | null>(null);
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
        setProfile(settings.engine);
        setFoxHistory(settings.fox_history);
        setTsumegoOptions(settings.tsumego);
        if (settings.engine) return;
        setAutoSetup(true);
        try {
          const configured = await backend.autoConfigureEngine();
          if (active) setProfile(configured);
        } catch (error) {
          if (!active) return;
          setAutoSetupFailed({ error });
          setSetupOpen(true);
        } finally {
          if (active) setAutoSetup(false);
        }
      })
      .catch(report);
    return () => {
      active = false;
    };
  }, []);

  // The tree is fetched only when its structure changes, never on plain navigation.
  const structure = document?.tree.structure_revision;
  useEffect(() => {
    if (structure === undefined) return;
    let active = true;
    backend
      .variationTree()
      .then((view) => {
        if (active && view.revision === structure) setTree(view);
      })
      .catch(report);
    return () => {
      active = false;
    };
  }, [structure]);

  // Rust ends a problem when another game is opened; while one is on, re-read its status.
  const framing = tsumego !== null;
  useEffect(() => {
    if (!framing) return;
    let active = true;
    backend
      .tsumegoStatus()
      .then((status) => {
        if (active && status.revision >= revision) setTsumego(status.active ? status : null);
      })
      .catch(report);
    return () => {
      active = false;
    };
  }, [framing, revision]);

  const step = (steps: number) => {
    backend.step(steps).catch(report);
  };
  const generateTsumego = async (options: TsumegoOptions) => {
    await backend.generateTsumego(options);
    setTsumegoOptions(options);
    setTsumegoOpen(false);
    const status = await backend.tsumegoStatus();
    setTsumego(status.active ? status : null);
  };
  const exitTsumego = () => {
    backend
      .exitTsumego()
      .then(() => setTsumego(null))
      .catch(report);
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
  const replay = useVariationPreview(backend, currentNode, revision, analysis, boardHover ?? bestPoint);
  const variation = boardHover ? replay : null;
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
  const liveTree = tree && document && tree.revision === document.tree.structure_revision ? tree : null;
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
  const boardNames = useMemo(
    () => (info && (info.black_name || info.white_name) ? { black: info.black_name ?? "", white: info.white_name ?? "" } : null),
    [info],
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
  const saveFile = async () => {
    const path = await backend.pickSavePath();
    if (path) await backend.save(path);
  };
  // Space and the chip resume a limit stop without changing the saved setting.
  const togglePondering = run.toggle;
  const livePondering = run.state?.pondering ?? pondering;
  useEffect(() => {
    backend.analysisLimits().then(setLimits).catch(report);
  }, []);
  // The backend records searches in the settings; read them back rather than guess.
  const refreshFoxHistory = () => {
    backend
      .settings()
      .then((settings) => setFoxHistory(settings.fox_history))
      .catch(report);
  };
  const jump = (nodeId: number) => {
    backend.goTo(nodeId).catch(report);
  };
  const pass = () => {
    if (board) backend.play({ type: "Pass", data: { color: board.to_play } }).catch(report);
  };
  const openFox = () => {
    setFoxUsed(true);
    setFoxOpen(true);
  };
  const current = document?.tree.current ?? null;
  const speed = analysis?.visits_per_second ?? null;
  const best = analysis && analysis.node_id === current ? analysis.candidates[0] : undefined;
  const wholeGameStatus = wholeGame.progress?.status.type;
  const bindings = menuBindings(
    {
      hasDocument: document !== null,
      hasBoard: board !== null,
      coordinates,
      ownership,
      wholeGameActive: wholeGameStatus === "Running" || wholeGameStatus === "Paused",
      hasBestMove: board !== null && best !== undefined,
      engineFailed: engine.type === "Failed" && profile !== null,
      engineConfigured: profile !== null,
      windows: WINDOWS,
      syncPhase: sync.status?.phase ?? "Stopped",
      syncOptions,
      tree: tree && document && tree.revision === document.tree.structure_revision ? tree : null,
      current,
    },
    {
      open: () => void openFile().catch(report),
      saveAs: () => void saveFile().catch(report),
      quit: () => void backend.quit().catch(report),
      toggleCoordinates: () => setCoordinates((value) => !value),
      setOwnership,
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
      openFox,
      startBoardSync: () => sync.start({ ...syncOptions, language: chinese ? "Cn" : "En" }),
      toggleSyncOption: (key) => setSyncOptions((options) => ({ ...options, [key]: !options[key] })),
      openTsumego: () => setTsumegoOpen(true),
      openBenchmark: () => setTuningOpen(true),
      openSettings: () => setSettingsOpen(true),
      openEngineSetup: () => setSetupOpen(true),
      restartEngine: () => {
        if (profile) backend.configureEngine(profile).catch(report);
      },
      openAbout: () => setAboutOpen(true),
    },
  );
  useShortcuts(bindings, !(setupOpen || strengthOpen || foxOpen || aboutOpen || tsumegoOpen || tuningOpen || settingsOpen));
  const frame = messages.frame;
  const engineLabel = useEngineName(backend, profile, frame.engineNames);
  const menus = useMemo(() => [...menusFor(WINDOWS), engineMenu(engineLabel)], [engineLabel]);
  const autoAnalyzing = quickRunning || wholeGameStatus === "Running";
  // Java's title: "[Black's winrate, lead, visits] engine speed - [黑] A vs [白] B (结果:…)".
  const title = windowTitle({
    app: messages.app.title,
    evaluation:
      analysis && analysis.node_id === current && analysis.visits > 0 ? evaluationText(analysis.winrate, analysis.score_lead, analysis.visits) : null,
    engine: engineLabel,
    speed:
      (livePondering && speed !== null ? `${Math.round(speed)} ${frame.title.speedUnit}` : "") +
      (engineLabel !== null && /b11|11b/i.test(engineLabel) ? ` · ${frame.title.b11} ` : ""),
    players:
      info && (info.black_name || info.white_name)
        ? `[${frame.title.black}] ${info.black_name ?? ""} vs [${frame.title.white}] ${info.white_name ?? ""}`
        : null,
    result: info?.result?.trim() ? `(${frame.title.result}${info.result.trim()})` : null,
    file: document?.file_name ?? null,
  });
  useEffect(() => {
    backend.setWindowTitle(title).catch(report);
  }, [title]);
  // A game that cannot be opened, picked or dropped, is reported by Rust; say so as Java does.
  useEffect(() => {
    let active = true;
    let id: number | undefined;
    backend
      .subscribeOpenFailed(setOpenFailure)
      .then((subscription) => {
        if (active) id = subscription;
        else backend.unsubscribe(subscription).catch(report);
      })
      .catch(report);
    return () => {
      active = false;
      if (id !== undefined) backend.unsubscribe(id).catch(report);
    };
  }, []);
  const reviewing = wholeGameStatus === "Running" || wholeGameStatus === "Paused";
  const reviewStatus: ReviewStatus | null =
    reviewing && wholeGame.progress
      ? { state: "running", text: messages.notes.statusRunning.replace("{done}", String(wholeGame.progress.completed)).replace("{total}", String(wholeGame.progress.total)) }
      : points.length > 1 && analysedCount === points.length
        ? { state: "done", text: messages.notes.statusDone }
        : analysedCount > 1
          ? { state: "partial", text: messages.notes.statusPartial.replace("{done}", String(analysedCount)).replace("{total}", String(points.length)) }
          : null;
  const summaryLines = board ? analysisSummary(points, currentIndex, board.last_move, engineLabel, info?.komi ?? 0, messages.notes) : [];
  // Java's status line: "engine · state · the key to press". First-run setup reads as starting.
  const shownEngine: EngineStatus = autoSetup && engine.type === "Stopped" ? { type: "Starting" } : engine;
  const engineDetail =
    shownEngine.type === "Failed" || shownEngine.type === "Restarting"
      ? shownEngine.data.reason
      : shownEngine.type === "Ready"
        ? shownEngine.data.version
        : undefined;
  const statusText = joinParts(
    " · ",
    engineLabel ?? frame.status.noEngine,
    shownEngine.type !== "Ready"
      ? messages.engine[shownEngine.type]
      : autoAnalyzing
        ? frame.status.autoAnalyzing
        : livePondering
          ? frame.status.analyzing
          : frame.status.paused,
    shownEngine.type === "Ready" && !autoAnalyzing ? (livePondering ? frame.status.spaceToPause : frame.status.spaceToResume) : null,
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
            <ToolButton className="jt-coach" label={frame.menu.aiCoach} title={frame.menu.aiCoachTip} enabled={false} />
            <ToolButton label={frame.menu.aiCommentary} enabled={false} />
          </>
        }
        tray={
          <>
            {tsumego ? <TsumegoChip status={tsumego} labels={messages.tsumego} onExit={exitTsumego} /> : null}
            {sync.status && sync.status.phase !== "Stopped" ? <BoardSyncChip status={sync.status} labels={messages.sync} onStop={sync.stop} /> : null}
            {tuning.status && !tuningOpen ? (
              <BenchmarkChip status={tuning.status} labels={messages.benchmark} onOpen={() => setTuningOpen(true)} onStop={tuning.cancel} />
            ) : null}
          </>
        }
      />
      <TopToolbar
        labels={frame.top}
        hasDocument={document !== null}
        hasBoard={board !== null}
        offMainLine={isEnabled(bindings.backToMainBranch)}
        komi={info?.komi ?? null}
        onOpen={() => void openFile().catch(report)}
        onSave={() => void saveFile().catch(report)}
        onFox={openFox}
        onStrength={() => setStrengthOpen(true)}
        onAutoSetup={() => setTuningOpen(true)}
        onFlash={() => wholeGame.start(1)}
        onBackMain={() => bindings.backToMainBranch?.run?.()}
        onPass={pass}
        onTsumego={() => setTsumegoOpen(true)}
      />
      <MainPanel
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
              <StatisticsBox board={board} points={points} currentIndex={currentIndex} labels={messages.info} />
            </div>
          ),
          graph: (
            <div className="graph-region">
              {quickRunning && quickPass ? <CurveProgress done={quickPass.completed} total={quickPass.total} labels={messages.graph} onStop={wholeGame.cancel} /> : null}
              {document ? <WinrateGraph graph={document.graph} analysis={analysis} mistakes={mistakes} labels={messages.graph} onJump={jump} /> : null}
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
            <span data-testid="engine-status" data-state={shownEngine.type} title={engineDetail ?? messages.engine[shownEngine.type]}>
              <StatusLine text={statusText} />
            </span>
          ),
          board: board ? (
            <Board
              board={board}
              analysis={analysis}
              coordinates={coordinates}
              ownership={ownership}
              maxCandidates={MAX_CANDIDATES}
              label={messages.board.label}
              variation={variation}
              names={boardNames}
              lastRank={lastRank}
              next={nextMoves}
              passLabel={messages.board.pass}
              emptyHint={emptyGame ? messages.board.emptyHint : null}
              onHoverCandidate={setBoardHover}
              onPlay={play}
            />
          ) : (
            <p className="board-message" data-testid="position-error">
              {document?.position_error ? messages.board.positionError.replace("{reason}", document.position_error) : messages.board.loading}
            </p>
          ),
          tree: (
            <MoveTree tree={tree} current={document?.tree.current ?? -1} line={line} boardSize={board?.width ?? 19} ranks={ranks} label={messages.tree.label} onJump={jump} />
          ),
          subBoard: board ? <SubBoard board={board} node={currentNode} revision={revision} variation={replay} label={messages.tree.subBoard} /> : null,
        }}
      />
      <BottomToolbar
        labels={frame.bottom}
        pondering={livePondering}
        hasBoard={board !== null}
        ownership={ownership}
        coordinates={coordinates}
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
        onToggleOwnership={() => setOwnership((value) => !value)}
        onToggleCoordinates={() => setCoordinates((value) => !value)}
        onStep={step}
      />
      {limitTips === "open" ? <LimitDialog labels={messages.limit} onClose={() => setLimitTips("done")} /> : null}
      {openFailure ? (
        <MessageBox
          title={frame.openFailed.title}
          lines={[fileName(openFailure.path), openFailure.reason]}
          ok={frame.openFailed.ok}
          testId="open-failed"
          onClose={() => setOpenFailure(null)}
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
      {foxUsed ? (
        <FoxDialog
          open={foxOpen}
          backend={backend}
          labels={messages.fox}
          history={foxHistory}
          onHistoryChanged={refreshFoxHistory}
          onClose={() => setFoxOpen(false)}
        />
      ) : null}
      {aboutOpen ? <AboutDialog name={messages.app.title} labels={messages.about} onClose={() => setAboutOpen(false)} /> : null}
      {settingsOpen && limits ? (
        <SettingsDialog
          values={{ limits, autoCurve }}
          labels={messages.prefs}
          onSave={async (values) => {
            setLimits(await backend.setAnalysisLimits(values.limits));
            if (values.autoCurve !== autoCurve) setAutoCurve(await backend.setAutoQuickAnalyzeOnLoad(values.autoCurve));
            setSettingsOpen(false);
          }}
          onClose={() => setSettingsOpen(false)}
        />
      ) : null}
      {tuningOpen ? (
        <BenchmarkDialog
          status={tuning.status}
          model={engineLabel}
          labels={messages.benchmark}
          onStart={tuning.start}
          onStop={tuning.cancel}
          onReset={tuning.reset}
          onClose={() => setTuningOpen(false)}
        />
      ) : null}
      {tsumegoOpen ? (
        <TsumegoDialog options={tsumegoOptions} labels={messages.tsumego} onConfirm={generateTsumego} onClose={() => setTsumegoOpen(false)} />
      ) : null}
      {setupOpen ? (
        <EngineSetup
          backend={backend}
          labels={messages.setup}
          current={profile}
          notice={autoSetupFailed ? autoSetupFailure(autoSetupFailed.error, messages.setup) : null}
          engineRunning={engine.type === "Starting" || engine.type === "Ready" || engine.type === "Restarting"}
          onConfigured={(configured) => {
            setProfile({ ...configured, config: configured.config ?? "" });
            setAutoSetupFailed(null);
            setSetupOpen(false);
          }}
          onClose={() => {
            setAutoSetupFailed(null);
            setSetupOpen(false);
          }}
        />
      ) : null}
    </main>
  );
}
