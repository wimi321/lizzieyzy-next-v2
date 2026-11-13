import { useCallback, useEffect, useState } from "react";
import type { Backend, FullLogStatus, Settings, UiLanguage, UpdateCheck } from "@/api/backend";
import type { MenuBinding } from "../menu/bindings";
import { DiagnosticsDialog, type DiagnosticsLabels } from "./DiagnosticsDialog";
import { UpdateDialog, type UpdateLabels } from "./UpdateDialog";

export interface ReleaseLabels {
  update: UpdateLabels;
  diagnostics: DiagnosticsLabels;
  /** The tray hint when the startup check found a new version. */
  newVersion: string;
}

type ReleaseBackend = Pick<
  Backend,
  | "setLanguage"
  | "setAutoCheckUpdates"
  | "checkUpdate"
  | "downloadUpdate"
  | "cancelUpdateDownload"
  | "updateDownloadStatus"
  | "installUpdate"
  | "subscribeUpdateDownloadProgress"
  | "unsubscribe"
  | "logDirectory"
  | "fullLogStatus"
  | "startFullLog"
  | "stopFullLog"
  | "exportDiagnostics"
  | "revealInFolder"
>;

function report(error: unknown) {
  console.error("Release command failed", error);
}

/** The interface language to show: the saved choice, or the system's. */
export function localeOf(language: UiLanguage, system: string): string {
  if (language === "ZhCn") return "zh-CN";
  if (language === "En") return "en";
  return system;
}

/**
 * P2-14 in the interface: 帮助 → 检查更新 and 诊断与日志, 停止完整日志, 设置 → 语言, the automatic
 * update check, and the tray hint when that check finds a new version.
 */
export function useRelease(backend: ReleaseBackend) {
  const [language, setLanguage] = useState<UiLanguage>("System");
  const [autoCheck, setAutoCheck] = useState(true);
  const [found, setFound] = useState<UpdateCheck | null>(null);
  const [updateOpen, setUpdateOpen] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [fullLog, setFullLog] = useState<FullLogStatus>({ enabled: false, remaining_seconds: 0 });

  const load = useCallback((settings: Settings) => {
    setLanguage(settings.language ?? "System");
    setAutoCheck(settings.auto_check_updates ?? true);
  }, []);

  // Rust ends the full trace on time; the menu follows it.
  const running = fullLog.enabled === true && (fullLog.remaining_seconds ?? 0) > 0;
  useEffect(() => {
    if (!running) return;
    const timer = window.setTimeout(() => setFullLog({ enabled: false, remaining_seconds: 0 }), (fullLog.remaining_seconds ?? 0) * 1000);
    return () => window.clearTimeout(timer);
  }, [running, fullLog]);

  const chooseLanguage = (next: UiLanguage) => {
    setLanguage(next);
    backend.setLanguage(next).then(setLanguage).catch(report);
  };
  const chooseAutoCheck = (enabled: boolean) => {
    setAutoCheck(enabled);
    backend.setAutoCheckUpdates(enabled).then(setAutoCheck).catch(report);
  };
  const startFullLog = () => void backend.startFullLog().then(setFullLog).catch(report);
  const stopFullLog = () => void backend.stopFullLog().then(setFullLog).catch(report);

  const openUpdate = () => {
    setUpdateOpen(true);
    setFound(null);
  };

  const bindings: Record<string, MenuBinding> = {
    checkUpdate: { run: openUpdate },
    diagnosticsAndLogs: { run: () => setDiagnosticsOpen(true) },
    stopFullTrace: { run: stopFullLog, enabled: running },
    languageDefault: { run: () => chooseLanguage("System"), checked: language === "System" },
    languageZH: { run: () => chooseLanguage("ZhCn"), checked: language === "ZhCn" },
    languageEN: { run: () => chooseLanguage("En"), checked: language === "En" },
  };

  const tray = (labels: ReleaseLabels) =>
    found?.available && !updateOpen ? (
      <button type="button" className="chip update-chip" data-testid="update-chip" onClick={openUpdate}>
        <span className="update-dot" aria-hidden="true" />
        {labels.newVersion.replace("{version}", found.version ?? "")}
      </button>
    ) : null;

  const dialogs = (labels: ReleaseLabels) => (
    <>
      {updateOpen ? (
        <UpdateDialog backend={backend} autoCheck={autoCheck} labels={labels.update} onAutoCheck={chooseAutoCheck} onClose={() => setUpdateOpen(false)} />
      ) : null}
      {diagnosticsOpen ? (
        <DiagnosticsDialog
          backend={backend}
          fullLog={fullLog}
          labels={labels.diagnostics}
          onStart={startFullLog}
          onStop={stopFullLog}
          onClose={() => setDiagnosticsOpen(false)}
        />
      ) : null}
    </>
  );

  return {
    language,
    locale: localeOf(language, navigator.language),
    autoCheck,
    chooseAutoCheck,
    load,
    /** What the startup check found; the tray shows it until the update window opens. */
    notice: setFound,
    bindings,
    tray,
    dialogs,
    dialogOpen: updateOpen || diagnosticsOpen,
  };
}
