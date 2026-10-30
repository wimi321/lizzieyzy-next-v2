import { useEffect, useMemo, useState } from "react";
import * as backend from "@/api/backend";
import type { EngineProfile, PointView } from "@/api/backend";
import { Board } from "./board/Board";
import { formatVisits } from "./board/format";
import { liveGraph } from "./graph/scale";
import { WinrateGraph } from "./graph/WinrateGraph";
import { type MenuDefinition, MenuBar } from "./layout/MenuBar";
import { getMessages } from "./locales";
import { CandidateTable } from "./panels/CandidateTable";
import { InfoPanel } from "./panels/InfoPanel";
import { MoveList } from "./panels/MoveList";
import { EngineSetup } from "./settings/EngineSetup";
import { connect, createStore, type Store, useAppState } from "./state/store";
import { EngineLight, JUMP, Toolbar } from "./toolbar/Toolbar";

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

/** LizzieYzy Next's window: menu and tool rows, info and graph left, board centre, moves and candidates right. */
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
  // A preview belongs to one position: once the current node changes it no longer applies.
  const [preview, setPreview] = useState<{ node: number; point: PointView } | null>(null);

  useEffect(() => {
    let active = true;
    backend
      .settings()
      .then((settings) => {
        if (!active) return;
        setPondering(settings.pondering);
        setProfile(settings.engine);
        // First run: nothing to analyse with until an engine is set up.
        if (!settings.engine) setSetupOpen(true);
      })
      .catch(report);
    return () => {
      active = false;
    };
  }, []);

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
  const previewPoint = preview && document && preview.node === document.tree.current ? preview.point : null;
  const points = useMemo(() => (document ? liveGraph(document.graph, analysis) : []), [document, analysis]);
  const currentIndex = document?.graph.current_index ?? 0;
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
  const jump = (nodeId: number) => {
    backend.goTo(nodeId).catch(report);
  };

  const menus: MenuDefinition[] = [
    {
      label: messages.menu.file,
      testId: "menu-file",
      items: [
        { kind: "action", label: messages.menu.open, testId: "menu-open", onSelect: () => void openFile().catch(report) },
        { kind: "action", label: messages.menu.save, testId: "menu-save", disabled: document === null, onSelect: () => void saveFile().catch(report) },
      ],
    },
    {
      label: messages.menu.view,
      testId: "menu-view",
      items: [
        { kind: "check", label: messages.menu.coordinates, testId: "menu-coordinates", checked: coordinates, onSelect: () => setCoordinates((value) => !value) },
        { kind: "check", label: messages.menu.ownership, testId: "menu-ownership", checked: ownership, onSelect: () => setOwnership((value) => !value) },
        { kind: "separator", key: "candidates" },
        ...CANDIDATE_LIMITS.map(
          (limit): MenuDefinition["items"][number] => ({
            kind: "check",
            label: `${messages.menu.candidates}: ${limit === 361 ? messages.board.candidatesAll : limit}`,
            testId: `menu-candidates-${limit}`,
            checked: maxCandidates === limit,
            onSelect: () => setMaxCandidates(limit),
          }),
        ),
      ],
    },
    {
      label: messages.menu.game,
      testId: "menu-game",
      items: [
        { kind: "action", label: messages.menu.first, testId: "menu-first", onSelect: () => step(-JUMP) },
        { kind: "action", label: messages.menu.previous, testId: "menu-previous", onSelect: () => step(-1) },
        { kind: "action", label: messages.menu.next, testId: "menu-next", onSelect: () => step(1) },
        { kind: "action", label: messages.menu.last, testId: "menu-last", onSelect: () => step(JUMP) },
      ],
    },
    {
      label: messages.menu.analysis,
      testId: "menu-analysis",
      items: [
        { kind: "action", label: pondering ? messages.menu.pause : messages.menu.resume, testId: "menu-pondering", onSelect: togglePondering },
      ],
    },
    {
      label: messages.menu.settings,
      testId: "menu-settings",
      items: [{ kind: "action", label: messages.menu.engine, testId: "menu-engine", onSelect: () => setSetupOpen(true) }],
    },
  ];

  return (
    <main className="app">
      <h1 className="visually-hidden">{messages.app.title}</h1>
      <MenuBar menus={menus}>
        <EngineLight status={engine} labels={messages.engine} />
      </MenuBar>
      <div className="tool-row">
        <Toolbar
          labels={messages.toolbar}
          moveNumber={board ? board.move_number : null}
          pondering={pondering}
          canSave={document !== null}
          onOpen={() => void openFile().catch(report)}
          onSave={() => void saveFile().catch(report)}
          onStep={step}
          onTogglePondering={togglePondering}
        />
        <span className="status" title={analysis ? messages.board.visits.replace("{visits}", String(analysis.visits)) : undefined}>
          {messages.board.analysis}
          <strong data-testid="analysis-move" data-node-id={analysis?.node_id} data-revision={analysis?.revision}>
            {analysis && board ? `${board.move_number} · ${formatVisits(analysis.visits)}` : messages.board.noAnalysis}
          </strong>
        </span>
        <span className="controls">
          <button
            type="button"
            className="toggle"
            aria-pressed={coordinates}
            data-testid="toggle-coordinates"
            onClick={() => setCoordinates((value) => !value)}
          >
            {messages.board.coordinates}
          </button>
          <button
            type="button"
            className="toggle"
            aria-pressed={ownership}
            data-testid="toggle-ownership"
            onClick={() => setOwnership((value) => !value)}
          >
            {messages.board.ownership}
          </button>
          <label className="status">
            {messages.board.candidates}
            <select data-testid="max-candidates" value={maxCandidates} onChange={(event) => setMaxCandidates(Number(event.target.value))}>
              {CANDIDATE_LIMITS.map((limit) => (
                <option key={limit} value={limit}>
                  {limit === 361 ? messages.board.candidatesAll : limit}
                </option>
              ))}
            </select>
          </label>
          <button type="button" className="toggle" data-testid="open-setup" onClick={() => setSetupOpen(true)}>
            {messages.setup.open}
          </button>
        </span>
      </div>
      <div className="workspace">
        <aside className="side side-left">
          <InfoPanel board={board} points={points} currentIndex={currentIndex} labels={messages.info} />
          <div className="graph-wrap">
            {document ? <WinrateGraph graph={document.graph} analysis={analysis} labels={messages.graph} onJump={jump} /> : null}
          </div>
          {document?.comment ? (
            <section className="comment" aria-label={messages.info.comment} data-testid="comment">
              <p>{document.comment}</p>
            </section>
          ) : null}
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
          <MoveList
            points={points}
            currentIndex={currentIndex}
            variationCount={document?.tree.variation_count ?? 0}
            labels={messages.tree}
            onJump={jump}
          />
          <CandidateTable
            analysis={analysis}
            toPlay={board?.to_play ?? null}
            maxCandidates={maxCandidates}
            labels={messages.table}
            onPreview={(point) => setPreview(point && document ? { node: document.tree.current, point } : null)}
            onPlay={play}
          />
        </aside>
      </div>
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
