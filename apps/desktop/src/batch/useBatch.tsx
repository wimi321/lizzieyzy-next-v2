import { useCallback, useEffect, useRef, useState } from "react";
import type { Backend, BatchAnalysisStatus, FlashAnalysisSettings, Settings, WholeGameScope } from "@/api/backend";
import { reasonOf } from "../everyday/useEveryday";
import { ConfirmDialog } from "../frame/ConfirmDialog";
import { MessageBox } from "../frame/MessageBox";
import { isEnabled, type MenuBinding, type MenuBindings } from "../menu/bindings";
import type { WholeGameControls } from "../review/useWholeGame";
import { BatchDialog, type BatchChoice } from "./BatchDialog";
import { BatchTable } from "./BatchTable";
import { FlashRangeDialog, FlashSettingsDialog } from "./FlashDialogs";
import type { BatchLabels, PresetLabels } from "./labels";

type BatchBackend = Pick<
  Backend,
  | "startBatchAnalysis"
  | "pauseBatchAnalysis"
  | "resumeBatchAnalysis"
  | "stopBatchAnalysis"
  | "addBatchAnalysisFiles"
  | "removeBatchAnalysisItem"
  | "moveBatchAnalysisItem"
  | "clearBatchAnalysis"
  | "batchAnalysisStatus"
  | "subscribeBatchAnalysisProgress"
  | "unsubscribe"
  | "setFlashAnalysis"
  | "pickOpenPaths"
  | "pickDirectory"
  | "openPath"
>;

export interface BatchView {
  hasDocument: boolean;
  /** The game on screen has changes that a batch would replace. */
  modified: boolean;
}

/** Java's flash defaults: one visit, keep what is already analysed. */
const FLASH_DEFAULTS: Required<FlashAnalysisSettings> = { visits: 1, always_override: false };

/** Entries that open, replace or change the game, which a running batch refuses. */
const GUARDED = new Set([
  "open", "newBoard", "save", "saveAs", "pasteSgf", "foxKifu", "setInfo", "setBoard", "delete", "deleteBranch", "setAsMain",
  "clearBoard", "undoDelete", "redoDelete", "autoAnalyze", "wholeGameDeepAnalysis", "clearAllLizzieCache", "clearThisLizzieCache",
  "clearAllLizzieBestmoves", "clearThisLizzieBestmoves", "readBoard", "tsumeGoMenu", "addBlack", "addWhite", "insertBlack", "insertWhite",
]);

type Dialog =
  | { kind: "options"; flash: boolean; files: string[] }
  | { kind: "unsaved"; start(): void }
  | { kind: "range" }
  | { kind: "settings" };

/**
 * P3-05: 分析 → 批量分析, 批量精析, 批量分析进度表, 停止自动(批量)分析, and the 精析 entries
 * (整盘速览, 局部精析, 全部分支精析, 精析设置). Rust owns the queue; this shows it and sends commands.
 */
export function useBatch(backend: BatchBackend, wholeGame: WholeGameControls, view: BatchView, labels: BatchLabels) {
  const [status, setStatus] = useState<BatchAnalysisStatus>({ phase: "Idle", items: [], current_index: null, reason: null });
  const [flash, setFlash] = useState(FLASH_DEFAULTS);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [tableOpen, setTableOpen] = useState(false);
  const [message, setMessage] = useState<string[] | null>(null);
  const events = useRef(0);
  const lastChoice = useRef<BatchChoice | null>(null);
  const lastFlash = useRef<WholeGameScope>({ type: "Line" });

  // Subscribe before reading, so no change falls between the two.
  useEffect(() => {
    let active = true;
    let heard = false;
    let id: number | undefined;
    backend
      .subscribeBatchAnalysisProgress((next) => {
        heard = true;
        events.current += 1;
        if (active) setStatus(next);
      })
      .then((subscription) => {
        id = subscription;
        if (!active) {
          void backend.unsubscribe(subscription);
          return null;
        }
        return backend.batchAnalysisStatus();
      })
      .then((initial) => {
        if (active && initial && !heard) setStatus(initial);
      })
      .catch((error: unknown) => console.error("Batch analysis subscription failed", error));
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend]);

  const load = useCallback((settings: Settings) => setFlash({ ...FLASH_DEFAULTS, ...settings.flash_analysis }), []);

  const fail = (error: unknown) => setMessage([reasonOf(error)]);
  // A reply is shown unless an event arrived after the command was sent: that event is newer.
  const send = (command: () => Promise<BatchAnalysisStatus>) => {
    const seen = events.current;
    command()
      .then((reply) => {
        if (events.current === seen) setStatus(reply);
      })
      .catch(fail);
  };

  const phase = status.phase ?? "Idle";
  const active = phase === "Running" || phase === "Paused";
  const items = status.items ?? [];
  const waiting = items.filter((item) => item.state?.type === "Waiting").length;
  const blocked = () => setMessage([labels.blocked]);

  const start = (choice: BatchChoice, files: readonly string[]) => {
    const go = () => {
      lastChoice.current = choice;
      setTableOpen(true);
      send(() =>
        backend.startBatchAnalysis({
          files: [...files],
          visits: choice.visits,
          scope: choice.scope,
          force: choice.force,
          output: { suffix: labels.suffix, directory: choice.directory },
        }),
      );
    };
    setDialog(view.modified ? { kind: "unsaved", start: go } : null);
    if (!view.modified) go();
  };

  const choose = (flashMode: boolean) => () =>
    void backend
      .pickOpenPaths()
      .then((files) => {
        if (files.length > 0) setDialog({ kind: "options", flash: flashMode, files });
      })
      .catch(fail);

  const runFlash = (scope: WholeGameScope) => {
    if (active) return blocked();
    lastFlash.current = scope;
    wholeGame.run({ visits: flash.visits, scope, force: flash.always_override });
  };

  const startStop = () => {
    if (active) return send(() => backend.stopBatchAnalysis());
    const choice = lastChoice.current;
    if (choice) start(choice, []);
    else setDialog({ kind: "options", flash: false, files: [] });
  };
  const pauseResume = () => send(() => (phase === "Paused" ? backend.resumeBatchAnalysis() : backend.pauseBatchAnalysis()));

  // While a batch runs, the whole-game controls steer the batch: its pause pauses the batch.
  const controls: WholeGameControls = active
    ? {
        ...wholeGame,
        start: blocked,
        run: blocked,
        pause: () => send(() => backend.pauseBatchAnalysis()),
        resume: () => send(() => backend.resumeBatchAnalysis()),
        cancel: () => send(() => backend.stopBatchAnalysis()),
      }
    : wholeGame;

  const wholeGameActive = wholeGame.progress?.status.type === "Running" || wholeGame.progress?.status.type === "Paused";
  const document = view.hasDocument;
  const bindings: Record<string, MenuBinding> = {
    batchAnalyze: { run: choose(false) },
    batchAnalysisMode: { run: choose(true) },
    batchAnalyzeTable: { run: () => setTableOpen(true) },
    stopAutoAnalyze: { run: active ? () => send(() => backend.stopBatchAnalysis()) : wholeGame.cancel, enabled: active || wholeGameActive },
    flashAnalyzeAllGame: { run: () => runFlash({ type: "Line" }), enabled: document },
    flashAnalyzePartGame: { run: () => (active ? blocked() : setDialog({ kind: "range" })), enabled: document },
    flashAnalyzeAllBranches: { run: () => runFlash({ type: "AllBranches" }), enabled: document },
    flashAnalyzeSettings: { run: () => setDialog({ kind: "settings" }) },
  };

  /** A running batch answers the entries it would refuse with an explanation, not silence. */
  const guard = (all: MenuBindings): MenuBindings => {
    if (!active) return all;
    const guarded: Record<string, MenuBinding> = { ...all };
    for (const [id, binding] of Object.entries(all)) {
      if ((GUARDED.has(id) || /^recent\d+$/.test(id)) && isEnabled(binding)) guarded[id] = { ...binding, run: blocked };
    }
    return guarded;
  };

  const tray = () => {
    if (!active) return null;
    const current = status.current_index ?? null;
    const item = current === null ? undefined : items[current];
    const progress = item?.state?.type === "Running" ? item.state.data.progress : null;
    const share = progress && progress.total > 0 ? progress.completed / progress.total : 0;
    return (
      <button type="button" className="chip progress-chip batch-chip" data-testid="batch-chip" data-state={phase} onClick={() => setTableOpen(true)}>
        <span className="progress-fill" style={{ width: `${share * 100}%` }} aria-hidden="true" />
        <span>{labels.chip.replace("{index}", String((current ?? 0) + 1)).replace("{count}", String(items.length))}</span>
      </button>
    );
  };

  const close = () => setDialog(null);
  const dialogs = (presets: PresetLabels) => (
    <>
      {tableOpen ? (
        <BatchTable
          status={status}
          labels={labels}
          onAdd={() =>
            void backend
              .pickOpenPaths()
              .then((files) => {
                if (files.length > 0) send(() => backend.addBatchAnalysisFiles(files));
              })
              .catch(fail)
          }
          onRemove={(id) => send(() => backend.removeBatchAnalysisItem(id))}
          onMove={(id, direction) => send(() => backend.moveBatchAnalysisItem(id, direction))}
          onClear={() => send(() => backend.clearBatchAnalysis())}
          onStartStop={startStop}
          onPauseResume={pauseResume}
          onOpen={(path) => void backend.openPath(path).catch(fail)}
          onClose={() => setTableOpen(false)}
        />
      ) : null}
      {dialog?.kind === "options" ? (
        <BatchDialog
          flash={dialog.flash ? { visits: flash.visits, force: flash.always_override } : null}
          files={dialog.files}
          waiting={waiting}
          labels={labels}
          presets={presets}
          pickFolder={() => backend.pickDirectory()}
          onStart={(choice) => start(choice, dialog.files)}
          onClose={close}
        />
      ) : null}
      {dialog?.kind === "unsaved" ? (
        <ConfirmDialog
          title={labels.unsavedTitle}
          lines={[labels.unsaved]}
          confirm={labels.continue}
          cancel={labels.cancel}
          testId="batch-unsaved"
          onConfirm={() => {
            setDialog(null);
            dialog.start();
          }}
          onCancel={close}
        />
      ) : null}
      {dialog?.kind === "range" ? (
        <FlashRangeDialog
          labels={labels}
          onStart={(scope) => {
            setDialog(null);
            runFlash(scope);
          }}
          onClose={close}
        />
      ) : null}
      {dialog?.kind === "settings" ? (
        <FlashSettingsDialog
          settings={flash}
          labels={labels}
          onSave={(next, redo) => {
            setDialog(null);
            backend
              .setFlashAnalysis(next)
              .then((saved) => {
                const applied = { ...FLASH_DEFAULTS, ...saved };
                setFlash(applied);
                if (redo && !active) {
                  wholeGame.run({ visits: applied.visits, scope: lastFlash.current, force: true });
                }
              })
              .catch(fail);
          }}
          onClose={close}
        />
      ) : null}
      {message ? <MessageBox title={labels.errorTitle} lines={message} ok={labels.ok} testId="batch-message" onClose={() => setMessage(null)} /> : null}
    </>
  );

  return { load, status, active, controls, bindings, guard, tray, dialogs, dialogOpen: dialog !== null || message !== null };
}
