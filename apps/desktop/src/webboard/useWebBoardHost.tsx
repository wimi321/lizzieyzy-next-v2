import { useEffect, useRef, useState } from "react";
import type { Backend, Settings, WebBoardStatus } from "@/api/backend";
import type { MenuBindings } from "../menu/bindings";
import { type WebBoardLabels, WebBoardChip } from "./WebBoardChip";

export type WebBoardBackend = Pick<Backend, "startWebBoard" | "stopWebBoard" | "webBoardStatus" | "setWebBoardOptions" | "forceEndWebTrial" | "subscribeWebBoardChanged" | "unsubscribe">;

interface Options {
  settings: Settings | null;
  windows: boolean;
  /** The menu row's text while the viewer runs (Java's 停止 Web 旁观). */
  stopLabel: string;
  /** A viewer's trial takes the board: a setup tool in hand is put down. */
  onTrialStart(): void;
}

const fill = (text: string, values: Record<string, string>) => text.replace(/\{(\w+)\}/g, (whole, key: string) => values[key] ?? whole);
const TOAST_MS = 3000;

/** Rust's typed refusal (`{ type: "WebBoard", data: { type: "Bind", data: { reason } } }`) as words. */
export function reasonOf(error: unknown): string {
  let value: unknown = error;
  for (let depth = 0; depth < 4 && value && typeof value === "object"; depth += 1) {
    const record = value as { data?: unknown; reason?: unknown; type?: unknown; message?: unknown };
    if (typeof record.reason === "string") return record.reason;
    if (typeof record.data === "string") return record.data;
    if (record.data && typeof record.data === "object") {
      value = record.data;
      continue;
    }
    if (typeof record.type === "string") return record.type;
    if (typeof record.message === "string") return record.message;
  }
  return error instanceof Error ? error.message : String(error);
}

/**
 * 同步 → 网页旁观 (P3-33) over P3-32's service: the menu rows, the strip's chip with its panel,
 * and whether a viewer's trial holds the board (the editing chip's own 结束试下 would be refused).
 */
export function useWebBoardHost(backend: WebBoardBackend, labels: WebBoardLabels, { settings, windows, stopLabel, onTrialStart }: Options) {
  const [status, setStatus] = useState<WebBoardStatus | null>(null);
  const [allowed, setAllowed] = useState<boolean | null>(null);
  const [open, setOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const hadTrial = useRef(false);

  useEffect(() => {
    let active = true;
    let id: number | undefined;
    void backend
      .subscribeWebBoardChanged?.((next) => active && setStatus(next))
      .then((subscription) => {
        if (active) id = subscription;
        else void backend.unsubscribe(subscription);
      });
    void backend
      .webBoardStatus?.()
      .then((next) => active && setStatus(next))
      .catch(() => undefined);
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend]);

  const running = status?.running === true;
  const since = status?.trial?.since ?? null;
  const trial = running && since !== null;

  useEffect(() => {
    if (trial && !hadTrial.current) onTrialStart();
    hadTrial.current = trial;
  }, [trial, onTrialStart]);

  // The panel's minutes: refreshed at once when a trial begins, then twice a minute.
  useEffect(() => {
    if (since === null) return;
    const first = setTimeout(() => setNow(Date.now()), 0);
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => {
      clearTimeout(first);
      clearInterval(timer);
    };
  }, [since]);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), TOAST_MS);
    return () => clearTimeout(timer);
  }, [toast]);

  const allowTrial = allowed ?? settings?.web_board?.allow_trial ?? true;
  const failed = (error: unknown) => setToast(fill(labels.failed, { reason: reasonOf(error) }));

  const start = () => {
    backend
      .startWebBoard?.()
      .then((next) => {
        setStatus(next);
        setOpen(true);
      })
      .catch((error: unknown) => setToast(fill(labels.startFailed, { reason: reasonOf(error) })));
  };
  const stop = () => {
    setOpen(false);
    backend
      .stopWebBoard?.()
      .then(() => setStatus((previous) => ({ ...previous, running: false, url: null, addresses: [], viewers: 0, trial: null })))
      .catch(failed);
  };
  const copy = (url: string) => {
    const clipboard = typeof navigator === "undefined" ? undefined : navigator.clipboard;
    if (!clipboard) {
      setToast(labels.copyFailed);
      return;
    }
    clipboard.writeText(url).then(
      () => setToast(labels.copiedUrl),
      () => setToast(labels.copyFailed),
    );
  };
  const toggleAllow = () => {
    const next = !allowTrial;
    backend
      .setWebBoardOptions?.({ allow_trial: next })
      .then(() => setAllowed(next))
      .catch(failed);
  };
  const forceEnd = () => void backend.forceEndWebTrial?.().catch(failed);

  const bindings: MenuBindings = {
    webBoardToggle: { run: running ? stop : start, ...(running ? { label: stopLabel } : {}) },
    webBoardCopyUrl: { run: () => status?.url && copy(status.url), enabled: running && Boolean(status?.url) },
    webBoardAllowTrial: { run: toggleAllow, checked: allowTrial },
    webBoardExitTrial: { run: forceEnd, enabled: trial },
  };

  const chip = (
    <>
      {running && status ? (
        <WebBoardChip
          status={status}
          labels={labels}
          open={open}
          trialMinutes={trial && since !== null ? Math.max(0, Math.floor((now - since) / 60_000)) : null}
          allowTrial={allowTrial}
          windows={windows}
          onToggle={() => setOpen((value) => !value)}
          onClose={() => setOpen(false)}
          onCopy={copy}
          onAllowTrial={toggleAllow}
          onForceEnd={forceEnd}
          onStop={stop}
        />
      ) : null}
      {toast ? (
        <span className="web-board-toast" role="status" data-testid="web-board-toast">
          {toast}
        </span>
      ) : null}
    </>
  );

  return { bindings, chip, trial };
}
