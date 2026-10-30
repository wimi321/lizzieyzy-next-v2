import { useEffect, useState } from "react";
import * as backend from "@/api/backend";
import type { EngineProfile, PointView } from "@/api/backend";
import { Board } from "./board/Board";
import { formatVisits } from "./board/format";
import { WinrateGraph } from "./graph/WinrateGraph";
import { getMessages } from "./locales";
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

  return (
    <main className="app">
      <header className="app-header">
        <h1 className="app-title">{messages.app.title}</h1>
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
        <EngineLight status={engine} labels={messages.engine} />
        <button type="button" className="toggle" data-testid="open-setup" onClick={() => setSetupOpen(true)}>
          {messages.setup.open}
        </button>
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
        </span>
      </header>
      <section className="board-area" data-testid="board-area" data-revision={revision} data-node-id={document?.tree.current}>
        {board ? (
          <Board
            board={board}
            analysis={analysis}
            coordinates={coordinates}
            ownership={ownership}
            maxCandidates={maxCandidates}
            label={messages.board.label}
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
      <section className="graph-area">
        {document ? (
          <WinrateGraph
            graph={document.graph}
            analysis={analysis}
            labels={messages.graph}
            onJump={(nodeId) => backend.goTo(nodeId).catch(report)}
          />
        ) : null}
      </section>
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
