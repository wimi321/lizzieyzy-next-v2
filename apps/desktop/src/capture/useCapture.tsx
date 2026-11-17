import { useCallback, useEffect, useRef, useState } from "react";
import type { Backend, CaptureSettings, DocumentView } from "@/api/backend";
import { ConfirmDialog } from "../frame/ConfirmDialog";
import type { MenuBindings } from "../menu/bindings";
import { type TsumegoLabels, TsumegoDialog } from "../tsumego/TsumegoDialog";
import type { useTsumego } from "../tsumego/useTsumego";
import { CaptureInstructions, CapturePermissionDialog, CaptureSettingsDialog } from "./CaptureDialogs";
import type { CaptureLabels } from "./CapturePage";
import { BOARD_SIZE_KEY } from "./geometry";
import { describe, fill } from "./values";

export type CaptureBackend = Pick<
  Backend,
  | "captureSettings"
  | "setCaptureSettings"
  | "capturePermission"
  | "requestCapturePermission"
  | "openCapturePermissionSettings"
  | "startScreenCapture"
  | "subscribeCaptureEnded"
  | "unsubscribe"
>;

type Dialog = { kind: "instructions" } | { kind: "settings"; settings: CaptureSettings } | { kind: "unsaved" } | { kind: "permission" };

const TOAST_MS = 4000;

/** Rust's refusal to start, in the words of this window. */
export function startError(error: unknown, labels: CaptureLabels): string | "permission" {
  const record = error as { type?: unknown; data?: { type?: unknown } } | null;
  if (record?.type === "ScreenCapture") {
    switch (record.data?.type) {
      case "PermissionMissing":
        return "permission";
      case "Busy":
        return labels.busy;
      case "Unsupported":
        return labels.unsupported;
      case "DisplayUnavailable":
        return labels.display;
    }
  }
  return fill(labels.failed, { reason: describe(error) });
}

interface Options {
  document: DocumentView | null;
  tsumego: ReturnType<typeof useTsumego>;
  tsumegoLabels: TsumegoLabels;
  /** Where the current game's size is left for the capture window. */
  storage?: Pick<Storage, "setItem"> | null;
}

/**
 * 抓取死活题 (P3-40) in the main window: the menu entry and Shift+T, the instruction window and
 * its settings, the questions before capturing, and what follows (the 死活题 dialog, which this
 * hook renders together with its new 屏幕抓取 button).
 */
export function useCapture(backend: CaptureBackend, labels: CaptureLabels, { document, tsumego, tsumegoLabels, storage = safeStorage() }: Options) {
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  // macOS asks for 屏幕录制 once per run; afterwards the explanation leads to System Settings.
  const asked = useRef(false);
  const { setOpen: openTsumego } = tsumego;

  useEffect(() => {
    let active = true;
    let id: number | undefined;
    void backend
      .subscribeCaptureEnded?.(({ outcome }) => {
        if (!active) return;
        // Finished: the problem is the new game; frame it at once. Cancelled: back to the steps (Java).
        if (outcome.type === "Finished") openTsumego(true);
        else if (outcome.type === "Cancelled") setDialog({ kind: "instructions" });
        else setToast(fill(labels.failed, { reason: outcome.data.reason }));
      })
      .then((subscription) => {
        if (active) id = subscription;
        else void backend.unsubscribe(subscription);
      });
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend, labels.failed, openTsumego]);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), TOAST_MS);
    return () => clearTimeout(timer);
  }, [toast]);

  const width = document?.board?.width ?? null;
  const begin = useCallback(async () => {
    setDialog(null);
    let permission = (await backend.capturePermission?.().catch(() => "NotNeeded" as const)) ?? "NotNeeded";
    if (permission === "Missing" && !asked.current) {
      asked.current = true;
      permission = (await backend.requestCapturePermission?.().catch(() => permission)) ?? permission;
    }
    if (permission === "Missing") {
      setDialog({ kind: "permission" });
      return;
    }
    try {
      storage?.setItem(BOARD_SIZE_KEY, String(width ?? 19));
    } catch {
      // A refused storage only loses the board size default.
    }
    try {
      await backend.startScreenCapture?.();
    } catch (error: unknown) {
      const reason = startError(error, labels);
      if (reason === "permission") setDialog({ kind: "permission" });
      else setToast(reason);
    }
  }, [backend, labels, storage, width]);

  const modified = document?.modified ?? false;
  const start = useCallback(() => {
    if (modified) setDialog({ kind: "unsaved" });
    else void begin();
  }, [modified, begin]);

  const openSettings = () => {
    backend
      .captureSettings?.()
      .then((settings) => setDialog({ kind: "settings", settings }))
      .catch((error: unknown) => setToast(fill(labels.saveFailed, { reason: describe(error) })));
  };
  const save = async (settings: Required<CaptureSettings>) => {
    await backend.setCaptureSettings?.(settings);
    setDialog({ kind: "instructions" });
  };
  const close = useCallback(() => setDialog(null), []);
  const closeTsumego = useCallback(() => openTsumego(false), [openTsumego]);

  const bindings: MenuBindings = {
    captureTsumeGo: { run: () => setDialog({ kind: "instructions" }) },
    // Shift+T: Java skips the instruction window.
    captureTsumeGoNow: { run: start },
  };

  const dialogs = (
    <>
      {tsumego.open ? (
        <TsumegoDialog
          options={tsumego.options}
          labels={tsumegoLabels}
          onConfirm={tsumego.generate}
          onClose={closeTsumego}
          onCapture={() => {
            openTsumego(false);
            setDialog({ kind: "instructions" });
          }}
        />
      ) : null}
      {dialog?.kind === "instructions" ? <CaptureInstructions labels={labels} onStart={start} onSettings={openSettings} onClose={close} /> : null}
      {dialog?.kind === "settings" ? <CaptureSettingsDialog settings={dialog.settings} labels={labels} onSave={save} onClose={() => setDialog({ kind: "instructions" })} /> : null}
      {dialog?.kind === "unsaved" ? (
        <ConfirmDialog
          title={labels.unsavedTitle}
          lines={[labels.unsavedMessage]}
          confirm={labels.continue}
          cancel={labels.cancel}
          testId="capture-unsaved"
          onConfirm={() => void begin()}
          onCancel={close}
        />
      ) : null}
      {dialog?.kind === "permission" ? (
        <CapturePermissionDialog
          labels={labels}
          onOpenSettings={() => void backend.openCapturePermissionSettings?.().catch((error: unknown) => setToast(fill(labels.failed, { reason: describe(error) })))}
          onRetry={() => void begin()}
          onClose={close}
        />
      ) : null}
      {toast ? (
        <span className="capture-toast" role="status" data-testid="capture-toast">
          {toast}
        </span>
      ) : null}
    </>
  );

  return { bindings, dialogs, dialogOpen: dialog !== null };
}

function safeStorage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}
