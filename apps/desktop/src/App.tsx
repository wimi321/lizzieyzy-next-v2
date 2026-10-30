import { useEffect, useMemo, useState } from "react";
import * as backend from "@/api/backend";
import type { ColorView, EngineProfile, FoxAccount, PointView, ProblemMoveSort, ProblemMovesQuery, VariationTreeView } from "@/api/backend";
import { Board } from "./board/Board";
import { formatVisits } from "./board/format";
import { FoxDialog } from "./fox/FoxDialog";
import { liveGraph } from "./graph/scale";
import { WinrateGraph } from "./graph/WinrateGraph";
import { getMessages } from "./locales";
import { CandidateTable } from "./panels/CandidateTable";
import { InfoPanel } from "./panels/InfoPanel";
import { MoveTree } from "./panels/MoveTree";
import { AutoAnalysis } from "./review/AutoAnalysis";
import { ProblemList } from "./review/ProblemList";
import { useProblemMoves } from "./review/useProblemMoves";
import { useWholeGame } from "./review/useWholeGame";
import { EngineSetup } from "./settings/EngineSetup";
import { connect, createStore, type Store, useAppState } from "./state/store";
import { JUMP, TopBar } from "./toolbar/Toolbar";

export { JUMP };

const CANDIDATE_LIMITS = [5, 10, 20, 361] as const;

const KEY_STEPS: Record<string, number> = {
  ArrowLeft: -1,
  ArrowRight: 1,
  Home: -JUMP,
  End: JUMP,
};

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
  const [foxHistory, setFoxHistory] = useState<FoxAccount[]>([]);
  // A preview belongs to one position: once the current node changes it no longer applies.
  const [preview, setPreview] = useState<{ node: number; point: PointView } | null>(null);
  const [tree, setTree] = useState<VariationTreeView | null>(null);
  const wholeGame = useWholeGame(backend);
  const [problemColor, setProblemColor] = useState<ColorView | null>(null);
  const [problemSort, setProblemSort] = useState<ProblemMoveSort>("LossDesc");
  const [notesTab, setNotesTab] = useState<"problems" | "comment">("problems");
  const problemQuery = useMemo<ProblemMovesQuery>(
    () => ({ minimum_rank: "Inaccuracy", color: problemColor, sort: problemSort }),
    [problemColor, problemSort],
  );
  const problems = useProblemMoves(backend, problemQuery, {
    revision,
    progress: wholeGame.progress?.completed ?? 0,
    live: analysis?.visits ?? 0,
  });

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

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      if (target instanceof HTMLInputElement || target instanceof HTMLSelectElement || target instanceof HTMLTextAreaElement) {
        return;
      }
      const steps = KEY_STEPS[event.key];
      if (steps === undefined || event.metaKey || event.ctrlKey || event.altKey) return;
      event.preventDefault();
      backend.step(steps).catch(report);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const board = document?.board ?? null;
  const points = useMemo(() => (document ? liveGraph(document.graph, analysis) : []), [document, analysis]);
  const line = useMemo(() => points.map((point) => point.node_id), [points]);
  const currentIndex = document?.graph.current_index ?? 0;
  const previewPoint = preview && document && preview.node === document.tree.current ? preview.point : null;
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

  return (
    <main className="app">
      <h1 className="visually-hidden">{messages.app.title}</h1>
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
        onOpenFox={() => setFoxOpen(true)}
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
            black={{ name: info?.black_name ?? null, rank: info?.black_rank ?? null }}
            white={{ name: info?.white_name ?? null, rank: info?.white_rank ?? null }}
            points={points}
            currentIndex={currentIndex}
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
        <section className="board-area" data-testid="board-area" data-revision={revision} data-node-id={document?.tree.current}>
          {board ? (
            <Board
              board={board}
              analysis={analysis}
              coordinates={coordinates}
              ownership={ownership}
              maxCandidates={maxCandidates}
              label={messages.board.label}
              preview={previewPoint}
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
      {foxOpen ? (
        <FoxDialog
          backend={backend}
          labels={messages.fox}
          history={foxHistory}
          onHistoryChanged={refreshFoxHistory}
          onClose={() => setFoxOpen(false)}
        />
      ) : null}
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
