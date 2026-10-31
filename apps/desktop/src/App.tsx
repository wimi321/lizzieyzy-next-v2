import { useEffect, useMemo, useRef, useState } from "react";
import * as backend from "@/api/backend";
import type { ColorView, EngineProfile, FoxAccount, PointView, ProblemMoveSort, ProblemMovesQuery, VariationTreeView } from "@/api/backend";
import { Board } from "./board/Board";
import { formatVisits } from "./board/format";
import { useVariationPreview } from "./board/useVariationPreview";
import { FoxDialog } from "./fox/FoxDialog";
import { liveGraph } from "./graph/scale";
import { WinrateGraph } from "./graph/WinrateGraph";
import { getMessages } from "./locales";
import { AboutDialog } from "./menu/AboutDialog";
import { menuBindings } from "./menu/bindings";
import { MenuBar } from "./menu/MenuBar";
import { useShortcuts } from "./menu/shortcuts";
import { engineMenu, MENUS } from "./menu/spec";
import { CandidateTable } from "./panels/CandidateTable";
import { InfoPanel } from "./panels/InfoPanel";
import { MoveTree } from "./panels/MoveTree";
import { AutoAnalysis, type AutoAnalysisHandle } from "./review/AutoAnalysis";
import { ProblemList } from "./review/ProblemList";
import { StrengthDialog } from "./review/StrengthDialog";
import { TIER_ORDER } from "./review/strength";
import { useProblemMoves } from "./review/useProblemMoves";
import { useStrength } from "./review/useStrength";
import { useWholeGame } from "./review/useWholeGame";
import { EngineSetup } from "./settings/EngineSetup";
import { connect, createStore, type Store, useAppState } from "./state/store";
import { JUMP, TopBar } from "./toolbar/Toolbar";

export { JUMP };

const CANDIDATE_LIMITS = [5, 10, 20, 361] as const;

/** A wheel notch (or this much trackpad travel) moves one move, as on Java's board. */
const WHEEL_STEP = 40;

/** "kata1-b18c384nbt-s9996604416-d4316597426.bin.gz" reads as "kata1-b18c384nbt". */
function engineName(model: string): string {
  const file = model.split(/[\\/]/).pop() ?? model;
  const base = file.replace(/\.(bin|txt)(\.gz)?$/i, "");
  return base.replace(/-s\d+-d\d+$/, "") || file;
}

function report(error: unknown) {
  console.error("Backend command failed", error);
}

/** LizzieYzy Next's window: info and graph left, board centre, candidates and the game tree right. */
export default function App({ store: provided }: { store?: Store } = {}) {
  const [store] = useState(() => provided ?? createStore());
  useEffect(() => connect(store, backend), [store]);

  const document = useAppState(store, (state) => state.document);
  const analysis = useAppState(store, (state) => state.analysis);
  const engine = useAppState(store, (state) => state.engine);
  const revision = useAppState(store, (state) => state.revision);
  const messages = getMessages(navigator.language);
  const [coordinates, setCoordinates] = useState(true);
  const [ownership, setOwnership] = useState(false);
  const [maxCandidates, setMaxCandidates] = useState<number>(10);
  const [pondering, setPondering] = useState(true);
  const [profile, setProfile] = useState<EngineProfile | null>(null);
  const [setupOpen, setSetupOpen] = useState(false);
  const [foxOpen, setFoxOpen] = useState(false);
  // Mounted on first use and then only hidden, so a minimized window keeps its list.
  const [foxUsed, setFoxUsed] = useState(false);
  const [strengthOpen, setStrengthOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  // State, not a ref: menu bindings are built during render and only call it later.
  const [autoAnalysis, setAutoAnalysis] = useState<AutoAnalysisHandle | null>(null);
  const wheel = useRef(0);
  const [foxHistory, setFoxHistory] = useState<FoxAccount[]>([]);
  // A preview belongs to one position: once the current node changes it no longer applies.
  const [preview, setPreview] = useState<{ node: number; point: PointView } | null>(null);
  const [boardHover, setBoardHover] = useState<PointView | null>(null);
  const [tree, setTree] = useState<VariationTreeView | null>(null);
  const wholeGame = useWholeGame(backend);
  const [problemColor, setProblemColor] = useState<ColorView | null>(null);
  const [problemSort, setProblemSort] = useState<ProblemMoveSort>("LossDesc");
  const [notesTab, setNotesTab] = useState<"problems" | "comment">("problems");
  const problemQuery = useMemo<ProblemMovesQuery>(
    () => ({ minimum_rank: "Inaccuracy", color: problemColor, sort: problemSort }),
    [problemColor, problemSort],
  );
  const reviewSignals = { revision, progress: wholeGame.progress?.completed ?? 0, live: analysis?.visits ?? 0 };
  const problems = useProblemMoves(backend, problemQuery, reviewSignals);
  const strength = useStrength(backend, reviewSignals);
  // Match rates join the player rows only once the estimate has enough moves to mean something.
  const strengthReport = strength?.report.has_enough_data ? strength.report : null;

  useEffect(() => {
    let active = true;
    backend
      .settings()
      .then((settings) => {
        if (!active) return;
        setPondering(settings.pondering);
        setProfile(settings.engine);
        setFoxHistory(settings.fox_history);
        // First run: nothing to analyse with until an engine is set up.
        if (!settings.engine) setSetupOpen(true);
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

  const step = (steps: number) => {
    backend.step(steps).catch(report);
  };

  const board = document?.board ?? null;
  const points = useMemo(() => (document ? liveGraph(document.graph, analysis) : []), [document, analysis]);
  const line = useMemo(() => points.map((point) => point.node_id), [points]);
  const currentIndex = document?.graph.current_index ?? 0;
  const previewPoint = preview && document && preview.node === document.tree.current ? preview.point : null;
  const variation = useVariationPreview(backend, document?.tree.current ?? null, revision, analysis, boardHover ?? previewPoint);
  const info = document?.game_info ?? null;
  // The mistake bars share the graph's line and indexing; anything else is stale.
  const mistakes = problems && problems.error_bar.length === points.length ? problems.error_bar : undefined;
  const analysed = points.some((point) => point.summary !== null);
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
  const togglePondering = () => {
    const next = !pondering;
    backend
      .setPondering(next)
      .then(() => setPondering(next))
      .catch(report);
  };
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
      openEngineSetup: () => setSetupOpen(true),
      restartEngine: () => {
        if (profile) backend.configureEngine(profile).catch(report);
      },
      openAbout: () => setAboutOpen(true),
    },
  );
  useShortcuts(bindings, !(setupOpen || strengthOpen || foxOpen || aboutOpen));
  const menus = useMemo(() => [...MENUS, engineMenu(profile ? engineName(profile.model) : null)], [profile]);

  return (
    <main className="app">
      <h1 className="visually-hidden">{messages.app.title}</h1>
      <MenuBar menus={menus} bindings={bindings} labels={messages.menu} label={messages.menuBar.label} />
      <TopBar
        labels={{
          ...messages.toolbar,
          coordinates: messages.board.coordinates,
          ownership: messages.board.ownership,
          settings: messages.setup.titleCurrent,
        }}
        engineLabels={messages.engine}
        moveNumber={board ? board.move_number : null}
        lastMoveNumber={points.length > 0 ? (points[points.length - 1]?.move_number ?? null) : null}
        pondering={pondering}
        analysis={
          analysis && board
            ? {
                nodeId: analysis.node_id,
                revision: analysis.revision,
                visits: analysis.visits,
                text: `${board.move_number} · ${formatVisits(analysis.visits)}`,
                title: messages.board.visits.replace("{visits}", String(analysis.visits)),
              }
            : null
        }
        noAnalysis={messages.board.noAnalysis}
        canSave={document !== null}
        coordinates={coordinates}
        ownership={ownership}
        engine={engine}
        extra={
          <AutoAnalysis
            ref={setAutoAnalysis}
            progress={wholeGame.progress}
            positions={points.length}
            labels={messages.review}
            onStart={wholeGame.start}
            onPause={wholeGame.pause}
            onResume={wholeGame.resume}
            onCancel={wholeGame.cancel}
          />
        }
        onOpen={() => void openFile().catch(report)}
        onSave={() => void saveFile().catch(report)}
        onOpenFox={openFox}
        onOpenStrength={() => setStrengthOpen(true)}
        onStep={step}
        onTogglePondering={togglePondering}
        onToggleCoordinates={() => setCoordinates((value) => !value)}
        onToggleOwnership={() => setOwnership((value) => !value)}
        onOpenSettings={() => setSetupOpen(true)}
      />
      <div className="workspace">
        <aside className="side side-left">
          <InfoPanel
            board={board}
            black={{
              name: info?.black_name ?? null,
              rank: info?.black_rank ?? null,
              match: strengthReport && strengthReport.black.sample_count > 0 ? strengthReport.black.match_rate : null,
            }}
            white={{
              name: info?.white_name ?? null,
              rank: info?.white_rank ?? null,
              match: strengthReport && strengthReport.white.sample_count > 0 ? strengthReport.white.match_rate : null,
            }}
            points={points}
            currentIndex={currentIndex}
            game={info}
            labels={messages.info}
          />
          <section className="section graph-section" aria-label={messages.graph.title}>
            <div className="section-header">
              <h2 className="section-title">{messages.graph.title}</h2>
              <span className="legend" aria-hidden="true">
                <span>{messages.graph.legendWinrate}</span>
                <span className="score">{messages.graph.legendScore}</span>
              </span>
            </div>
            <div className="graph-wrap">
              {document ? (
                <WinrateGraph graph={document.graph} analysis={analysis} mistakes={mistakes} labels={messages.graph} onJump={jump} />
              ) : null}
            </div>
          </section>
          <section className="section notes-section">
            <div className="tabs" role="tablist">
              <button
                type="button"
                role="tab"
                aria-selected={notesTab === "problems"}
                data-testid="tab-problems"
                onClick={() => setNotesTab("problems")}
              >
                {messages.problems.title}
                {problems && problems.entries.length > 0 ? <span className="tab-count">{problems.entries.length}</span> : null}
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={notesTab === "comment"}
                data-testid="tab-comment"
                onClick={() => setNotesTab("comment")}
              >
                {messages.problems.comment}
                {document?.comment ? <span className="tab-dot" aria-hidden="true" /> : null}
              </button>
            </div>
            {notesTab === "problems" ? (
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
            ) : (
              <div className="comment" data-testid="comment">
                {document?.comment ? <p>{document.comment}</p> : <p className="panel-empty">{messages.problems.noComment}</p>}
              </div>
            )}
          </section>
        </aside>
        <section
          className="board-area"
          data-testid="board-area"
          data-revision={revision}
          data-node-id={document?.tree.current}
          onWheel={(event) => {
            // Lines (Firefox, some mice) count as whole notches.
            wheel.current += event.deltaMode === 0 ? event.deltaY : Math.sign(event.deltaY) * WHEEL_STEP;
            if (Math.abs(wheel.current) < WHEEL_STEP) return;
            const steps = Math.sign(wheel.current);
            wheel.current = 0;
            step(steps);
          }}
        >
          {board ? (
            <Board
              board={board}
              analysis={analysis}
              coordinates={coordinates}
              ownership={ownership}
              maxCandidates={maxCandidates}
              label={messages.board.label}
              preview={previewPoint}
              variation={variation}
              onHoverCandidate={setBoardHover}
              onPlay={play}
            />
          ) : (
            <p className="board-message" data-testid="position-error">
              {document?.position_error
                ? messages.board.positionError.replace("{reason}", document.position_error)
                : messages.board.loading}
            </p>
          )}
        </section>
        <aside className="side side-right">
          <CandidateTable
            analysis={analysis}
            toPlay={board?.to_play ?? null}
            maxCandidates={maxCandidates}
            candidateLimits={CANDIDATE_LIMITS}
            labels={{ ...messages.table, limit: messages.board.candidates, limitAll: messages.board.candidatesAll }}
            onMaxCandidates={setMaxCandidates}
            onPreview={(point) => setPreview(point && document ? { node: document.tree.current, point } : null)}
            onPlay={play}
          />
          <MoveTree
            tree={tree}
            current={document?.tree.current ?? -1}
            line={line}
            boardSize={board?.width ?? 19}
            variationCount={document?.tree.variation_count ?? 0}
            labels={messages.tree}
            onJump={jump}
          />
        </aside>
      </div>
      {strengthOpen ? (
        <StrengthDialog
          view={strength}
          progress={wholeGame.progress}
          black={{ name: info?.black_name || messages.info.black, rank: info?.black_rank ?? null }}
          white={{ name: info?.white_name || messages.info.white, rank: info?.white_rank ?? null }}
          chinese={navigator.language.toLowerCase().startsWith("zh")}
          labels={messages.strength}
          tierNames={TIER_ORDER.map((rank) => messages.problems.ranks[rank])}
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
      {setupOpen ? (
        <EngineSetup
          backend={backend}
          labels={messages.setup}
          current={profile}
          engineRunning={engine.type === "Starting" || engine.type === "Ready" || engine.type === "Restarting"}
          onConfigured={(configured) => {
            setProfile({ ...configured, config: configured.config ?? "" });
            setSetupOpen(false);
          }}
          onClose={() => setSetupOpen(false)}
        />
      ) : null}
    </main>
  );
}
