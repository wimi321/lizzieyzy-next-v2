import { useEffect, useState } from "react";
import type { Backend, UpdateCheck, UpdateDownloadStatus } from "@/api/backend";

export interface UpdateLabels {
  title: string;
  checking: string;
  latest: string;
  available: string;
  published: string;
  notes: string;
  notConfigured: string;
  failed: string;
  download: string;
  cancel: string;
  progress: string;
  install: string;
  installing: string;
  autoCheck: string;
  autoCheckHint: string;
  close: string;
}

type UpdateBackend = Pick<
  Backend,
  "checkUpdate" | "downloadUpdate" | "cancelUpdateDownload" | "updateDownloadStatus" | "installUpdate" | "subscribeUpdateDownloadProgress" | "unsubscribe"
>;

type Check = { kind: "checking" } | { kind: "done"; result: UpdateCheck } | { kind: "notConfigured" } | { kind: "failed"; message: string };

/** A typed backend error says which; anything else is shown as text. */
function failure(error: unknown): Check {
  if (typeof error === "object" && error !== null && "type" in error) {
    const typed = error as { type: string; data?: unknown };
    if (typed.type === "UpdateSigningNotConfigured") return { kind: "notConfigured" };
    if (typeof typed.data === "string") return { kind: "failed", message: typed.data };
  }
  return { kind: "failed", message: error instanceof Error ? error.message : String(error) };
}

const mb = (bytes: number | null | undefined) => ((bytes ?? 0) / 1_048_576).toFixed(1);

/** Progress events and command replies can arrive in either order: within a phase, never go back. */
function newer(previous: UpdateDownloadStatus | null, next: UpdateDownloadStatus): UpdateDownloadStatus {
  return previous && previous.phase === next.phase && (previous.downloaded_bytes ?? 0) > (next.downloaded_bytes ?? 0) ? previous : next;
}

/**
 * 帮助 → 检查更新 (Java's update window): checks when opened, then offers the new version's
 * download and installation. Signature checks are Rust's; a refused package never installs.
 */
export function UpdateDialog({
  backend,
  autoCheck,
  labels,
  onAutoCheck,
  onClose,
}: {
  backend: UpdateBackend;
  autoCheck: boolean;
  labels: UpdateLabels;
  onAutoCheck(enabled: boolean): void;
  onClose(): void;
}) {
  const [check, setCheck] = useState<Check>({ kind: "checking" });
  const [download, setDownload] = useState<UpdateDownloadStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  // Subscribe before reading the status, so no progress falls between the two; then check.
  useEffect(() => {
    let active = true;
    let id: number | undefined;
    backend
      .subscribeUpdateDownloadProgress((status) => active && setDownload((previous) => newer(previous, status)))
      .then((subscription) => {
        if (active) id = subscription;
        else void backend.unsubscribe(subscription);
        return backend.updateDownloadStatus();
      })
      .then((status) => active && setDownload((previous) => newer(previous, status)))
      .catch(() => {});
    backend
      .checkUpdate()
      .then((result) => active && setCheck({ kind: "done", result }))
      .catch((error: unknown) => active && setCheck(failure(error)));
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend]);

  const act = (action: () => Promise<UpdateDownloadStatus>) => {
    setBusy(true);
    action()
      .then((status) => setDownload((previous) => newer(previous, status)))
      .catch((error: unknown) => setCheck(failure(error)))
      .finally(() => setBusy(false));
  };

  const phase = download?.phase ?? "Idle";
  const downloading = phase === "Downloading";
  const ready = phase === "Downloaded";
  const installing = phase === "Installing" || phase === "Completed";
  const result = check.kind === "done" ? check.result : null;
  const state =
    check.kind === "checking"
      ? labels.checking
      : check.kind === "notConfigured"
        ? labels.notConfigured
        : check.kind === "failed"
          ? labels.failed.replace("{reason}", check.message)
          : result?.available
            ? labels.available.replace("{version}", result.version ?? "")
            : labels.latest;

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog update-dialog" role="dialog" aria-modal="true" aria-labelledby="update-title" data-testid="update-dialog">
        <header className="dialog-header">
          <h2 id="update-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <p className="update-state" data-testid="update-state" data-state={check.kind === "done" ? (result?.available ? "available" : "latest") : check.kind}>
          {state}
        </p>
        {result?.available ? (
          <div className="update-release">
            {result.published_at ? <p className="update-date">{labels.published.replace("{date}", result.published_at.slice(0, 10))}</p> : null}
            {result.notes ? (
              <>
                <h3 className="dialog-subhead">{labels.notes}</h3>
                <pre className="update-notes" data-testid="update-notes">
                  {result.notes}
                </pre>
              </>
            ) : null}
          </div>
        ) : null}
        {downloading ? (
          <div className="tools-progress" data-testid="update-progress">
            <div className="tools-progress-bar">
              <div style={{ width: download?.total_bytes ? `${Math.min(100, ((download.downloaded_bytes ?? 0) / download.total_bytes) * 100)}%` : "0%" }} />
            </div>
            <span>{labels.progress.replace("{done}", mb(download?.downloaded_bytes)).replace("{total}", mb(download?.total_bytes))}</span>
            <button type="button" className="secondary-button" data-testid="update-cancel" disabled={busy} onClick={() => act(() => backend.cancelUpdateDownload())}>
              {labels.cancel}
            </button>
          </div>
        ) : null}
        {phase === "Failed" && download?.error ? (
          <div className="dialog-error" role="alert" data-testid="update-error">
            <pre>{download.error}</pre>
          </div>
        ) : null}
        <label className="settings-check update-auto">
          <input type="checkbox" data-testid="update-auto-check" checked={autoCheck} onChange={(event) => onAutoCheck(event.target.checked)} />
          <span>{labels.autoCheck}</span>
        </label>
        <p className="settings-hint">{labels.autoCheckHint}</p>
        <footer className="dialog-footer">
          {ready || installing ? (
            <button type="button" className="primary-button" data-testid="update-install" disabled={busy || installing} onClick={() => act(() => backend.installUpdate())}>
              {installing ? labels.installing : labels.install}
            </button>
          ) : result?.available ? (
            <button type="button" className="primary-button" data-testid="update-download" disabled={busy || downloading} onClick={() => act(() => backend.downloadUpdate())}>
              {labels.download}
            </button>
          ) : null}
        </footer>
      </section>
    </div>
  );
}
