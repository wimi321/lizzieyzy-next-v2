import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import type { Backend, BoardView, MarkupView } from "@/api/backend";
import type { CoachBestMove, CoachDecision, CoachHint, CoachMoveRated, CoachSetup, CoachView, HumanSlModelStatus } from "../api/coach";
import type { GameView } from "../api/game";
import type { AssetDownloadStatus } from "../api/generated/AssetDownloadStatus";
import type { TreeView } from "../api/generated/TreeView";
import type { Display } from "../display/settings";
import { reasonOf } from "../everyday/useEveryday";
import { ConfirmDialog } from "../frame/ConfirmDialog";
import { ToolButton } from "../frame/ToolButton";
import type en from "../locales/en.json";
import type { MenuBindings } from "../menu/bindings";
import { CoachDialog } from "./CoachDialog";
import { CoachPanel } from "./CoachPanel";
import { hintMarks } from "./rank";

type CoachBackend = Pick<
  Backend,
  | "startCoach"
  | "coachHint"
  | "coachDecision"
  | "coachReview"
  | "humanslModelStatus"
  | "importHumanslModel"
  | "pauseAssetDownload"
  | "startAssetDownload"
  | "cancelAssetDownload"
  | "subscribeCoachUpdated"
  | "subscribeCoachMoveRated"
  | "subscribeCoachSummary"
  | "subscribeGameUpdated"
  | "subscribeAssetDownloadProgress"
  | "unsubscribe"
  | "pickEngineFile"
  | "takeBack"
  | "resignGame"
  | "stopGame"
>;

type Messages = Pick<typeof en, "coach" | "play" | "board">;
const MODEL = "human-sl";

export interface CoachPlace {
  hasStones: boolean;
  board: BoardView | null;
  tree: TreeView | null;
  pass(): void;
  jump(node: number): void;
}

/**
 * P3-13: AI 陪练. Rust plays the HumanSL side, rates moves and keeps the counts; this sets the
 * game up, fetches the model, and shows the coach panel, hints, the 恶手 choice and the summary.
 */
export function useCoach(backend: CoachBackend, messages: Messages, place: CoachPlace) {
  const labels = messages.coach;
  const [coach, setCoach] = useState<CoachView | null>(null);
  const [game, setGame] = useState<GameView | null>(null);
  const [ratings, setRatings] = useState<CoachMoveRated[]>([]);
  const [hint, setHint] = useState<CoachHint | null>(null);
  const [best, setBest] = useState<CoachBestMove | null>(null);
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<HumanSlModelStatus | null>(null);
  const [download, setDownload] = useState<AssetDownloadStatus | null>(null);
  const [preparing, setPreparing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastSetup, setLastSetup] = useState<CoachSetup | null>(null);
  const [confirm, setConfirm] = useState<"resign" | "end" | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const pending = useRef<CoachSetup | null>(null);
  // Rust's events win: a reply only counts if no event of its kind came after the request.
  const coachEvents = useRef(0);
  const downloadEvents = useRef(0);

  const describe = useCallback(
    (failure: unknown): string => {
      if (typeof failure === "object" && failure !== null && "type" in failure) {
        let error = failure as { type: string; data?: unknown };
        if ((error.type === "Coach" || error.type === "Game") && typeof error.data === "object" && error.data !== null) error = error.data as typeof error;
        if (error.type === "UnsupportedVersion") return labels.errors.UnsupportedVersion.replace("{version}", String(error.data ?? ""));
        if (error.type === "HumanModelLoadFailed") return labels.errors.HumanModelLoadFailed.replace("{reason}", reasonOf(error.data));
        const known = (labels.errors as Record<string, string>)[error.type] ?? (messages.play.errors as Record<string, string>)[error.type];
        if (known) return known;
      }
      return reasonOf(failure);
    },
    [labels, messages.play],
  );

  const start = useCallback(
    (setup: CoachSetup) => {
      if (!backend.startCoach) return;
      pending.current = null;
      setPreparing(true);
      setError(null);
      setRatings([]);
      setHint(null);
      setBest(null);
      const sent = coachEvents.current;
      backend
        .startCoach(setup)
        .then((view) => {
          if (coachEvents.current === sent) setCoach(view);
          setLastSetup(setup);
          setOpen(false);
        })
        .catch((failure: unknown) => setError(describe(failure)))
        .finally(() => setPreparing(false));
    },
    [backend, describe],
  );

  const readStatus = useCallback(() => {
    if (!backend.humanslModelStatus) return Promise.resolve(null);
    return backend
      .humanslModelStatus()
      .then((next) => {
        setStatus(next);
        return next;
      })
      .catch((failure: unknown) => {
        setStatus({ type: "Missing" });
        setError(describe(failure));
        return null;
      });
  }, [backend, describe]);

  useEffect(() => {
    let active = true;
    const ids: Promise<number>[] = [];
    if (backend.subscribeCoachUpdated)
      ids.push(
        backend.subscribeCoachUpdated((view) => {
          if (!active) return;
          coachEvents.current += 1;
          setCoach((current) => ({ ...view, summary: view.summary ?? current?.summary ?? null }));
          if (!view.awaiting_decision) setBest(null);
        }),
      );
    if (backend.subscribeCoachMoveRated)
      ids.push(
        backend.subscribeCoachMoveRated((rating) => {
          if (!active) return;
          setRatings((current) => [...current.filter((old) => old.node_id !== rating.node_id), rating]);
          setHint(null);
        }),
      );
    if (backend.subscribeCoachSummary) ids.push(backend.subscribeCoachSummary((summary) => active && setCoach((current) => (current ? { ...current, summary } : current))));
    if (backend.subscribeGameUpdated) ids.push(backend.subscribeGameUpdated((view) => active && setGame(view)));
    ids.push(
      backend.subscribeAssetDownloadProgress((progress) => {
        if (!active || progress.asset_id !== MODEL) return;
        downloadEvents.current += 1;
        setDownload(progress);
        if (progress.phase === "Completed") {
          void readStatus().then((next) => {
            if (next?.type === "Ready" && pending.current) start(pending.current);
          });
        }
        if (progress.phase === "Failed") setError(progress.error ?? null);
      }),
    );
    return () => {
      active = false;
      for (const id of ids) void id.then((value) => backend.unsubscribe(value)).catch(() => undefined);
    };
  }, [backend, readStatus, start]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 2600);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const active = coach?.active === true;
  const shown = coach !== null && (active || coach.summary !== null && coach.summary !== undefined);
  const mode = coach?.setup?.mode;
  const hides = active && mode !== "LiveAnalysis";

  const openDialog = () => {
    setOpen(true);
    setError(null);
    setStatus(null);
    void readStatus();
  };
  const askHint = () => {
    if (!backend.coachHint) return;
    void backend
      .coachHint()
      .then(setHint)
      .catch((failure: unknown) => setToast(describe(failure)));
  };
  const decide = (decision: CoachDecision) => {
    if (!backend.coachDecision) return;
    void backend
      .coachDecision(decision)
      .then((answer) => {
        if (decision === "ShowBest" && answer.type === "Exact") setBest(answer.data);
        else setBest(null);
      })
      .catch((failure: unknown) => setToast(describe(failure)));
  };
  const close = () => {
    setCoach(null);
    setRatings([]);
    setHint(null);
    setBest(null);
  };

  const bindings: MenuBindings = {
    newHumanSlGame: { run: openDialog, enabled: !active && !preparing },
    ...(hides
      ? {
          togglePonder: { run: () => setToast(labels.hidden) },
          showHeatmap: { run: () => (mode === "LiveFeedback" ? askHint() : setToast(labels.hidden)) },
        }
      : {}),
  };

  /** The menu strip's AI 陪练 button: AI 陪练, 准备中… or 陪练中, as Java labels it. */
  const button = (texts: { aiCoach: string; aiCoachTip: string }) => (
    <ToolButton
      className="jt-coach"
      label={preparing ? labels.buttonPreparing : active ? labels.buttonActive : texts.aiCoach}
      title={texts.aiCoachTip}
      pressed={active}
      testId="coach-button"
      onClick={active ? undefined : openDialog}
    />
  );

  const board = place.board;
  const markup = (existing: readonly MarkupView[] | undefined): readonly MarkupView[] | undefined => {
    if (!board || (!hint && !best)) return existing;
    const marks = [...hintMarks(hint, board.width, board.height), ...hintMarks(best ? { type: "Exact", data: best } : null, board.width, board.height)];
    return [...(existing ?? []), ...marks];
  };

  const panel = (): ReactNode =>
    shown && coach ? (
      <CoachPanel
        view={coach}
        game={game}
        board={board}
        ratings={ratings}
        hint={hint}
        best={best}
        labels={labels}
        play={messages.play}
        passLabel={messages.board.pass}
        actions={{
          hint: askHint,
          closeHint: () => setHint(null),
          takeBack: () => void backend.takeBack?.().catch((failure: unknown) => setToast(describe(failure))),
          pass: place.pass,
          resign: () => setConfirm("resign"),
          end: () => setConfirm("end"),
          decide,
          reviewMove: (node) => {
            const line = place.tree?.main_line ?? [];
            const at = line.indexOf(node);
            place.jump(at > 0 ? line[at - 1]! : node);
          },
          review: () => {
            void (backend.coachReview?.() ?? Promise.resolve()).then(close).catch((failure: unknown) => setToast(describe(failure)));
          },
          again: () => {
            close();
            openDialog();
          },
        }}
      />
    ) : null;

  const dialogs = () => (
    <>
      {open ? (
        <CoachDialog
          saved={lastSetup}
          status={status}
          download={download}
          preparing={preparing}
          error={error}
          canContinue={place.hasStones}
          labels={labels}
          onStart={start}
          onDownload={(setup) => {
            pending.current = setup;
            setError(null);
            const sent = downloadEvents.current;
            void backend
              .startAssetDownload(MODEL)
              .then((status) => downloadEvents.current === sent && setDownload(status))
              .catch((failure: unknown) => setError(describe(failure)));
          }}
          onPause={() => void backend.pauseAssetDownload?.().then(setDownload).catch((failure: unknown) => setError(describe(failure)))}
          onCancelDownload={() => {
            pending.current = null;
            void backend.cancelAssetDownload().then(setDownload).catch((failure: unknown) => setError(describe(failure)));
          }}
          onImport={() =>
            void backend
              .pickEngineFile("model")
              .then((path) => (path && backend.importHumanslModel ? backend.importHumanslModel(path).then(setStatus) : undefined))
              .catch((failure: unknown) => setError(describe(failure)))
          }
          onClose={() => setOpen(false)}
        />
      ) : null}
      {confirm ? (
        <ConfirmDialog
          title={confirm === "resign" ? labels.resign : labels.end}
          lines={[confirm === "resign" ? labels.confirmResign : labels.confirmEnd]}
          confirm={confirm === "resign" ? labels.resign : labels.end}
          cancel={labels.cancel}
          testId="coach-confirm"
          onConfirm={() => {
            const which = confirm;
            setConfirm(null);
            void (which === "resign" ? backend.resignGame : backend.stopGame)?.().catch((failure: unknown) => setToast(describe(failure)));
          }}
          onCancel={() => setConfirm(null)}
        />
      ) : null}
      {toast ? (
        <div className="coach-toast" role="status" data-testid="coach-toast">
          {toast}
        </div>
      ) : null}
    </>
  );

  return {
    active,
    /** Live analysis is hidden in 边下边提醒 and 实战后复盘. */
    hidesAnalysis: hides,
    /** What the display shows during the coach game: no analysis where it is hidden, and room for the panel. */
    shown: (display: Display): Display =>
      shown ? { ...display, show_list_pane: true, show_sub_board: false, ...(hides ? { show_winrate_graph: false, show_hover_variation: false, estimate_location: "Off" as const } : {}) } : display,
    bindings,
    button,
    markup,
    panel,
    dialogs,
    dialogOpen: open || confirm !== null,
  };
}
