import { type ReactNode, useCallback, useState } from "react";
import type { AutoSetupError, Backend, EngineRules, EngineStatus, JavaImportPreview, Settings } from "@/api/backend";
import { MessageBox } from "../frame/MessageBox";
import type { MenuBinding } from "../menu/bindings";
import { useEngineName } from "../frame/useEngineName";
import { EngineSetup, type SetupLabels } from "../settings/EngineSetup";
import { JavaImportDialog, type JavaImportLabels, RulesDialog, type RulesLabels } from "./EngineDialogs";
import { EngineList, type EngineListLabels } from "./EngineList";
import { describe, type Engine, type EngineOptions, engineName, engineState, ENGINE_DEFAULTS, nameFromModel } from "./model";

export interface EngineLabels {
  list: EngineListLabels;
  rules: RulesLabels;
  javaImport: JavaImportLabels;
  nameLabel: string;
  addTitle: string;
  editTitle: string;
  failedTitle: string;
  ok: string;
}

type EnginesBackend = Pick<
  Backend,
  | "settings"
  | "autoConfigureEngine"
  | "detectEngines"
  | "tryConfigureEngine"
  | "pickEngineFile"
  | "addEngine"
  | "updateEngine"
  | "removeEngine"
  | "reorderEngines"
  | "selectMainEngine"
  | "restartEngine"
  | "closeEngine"
  | "setEngineSettings"
  | "pickDirectory"
  | "previewJavaImport"
  | "confirmJavaImport"
  | "engineDisplayName"
>;

type Setup = { kind: "main" } | { kind: "add" } | { kind: "edit"; id: string };

/** Why automatic setup gave up, for the setup dialog it opens instead. */
function autoSetupFailure(error: unknown, labels: { autoNoPrograms: string; autoNoModels: string; autoAllFailed: string }): string {
  const failure =
    typeof error === "object" && error !== null && "type" in error && error.type === "AutoSetup" && "data" in error ? (error.data as AutoSetupError) : null;
  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
  if (failure?.type === "NoPrograms") return labels.autoNoPrograms;
  if (failure?.type === "NoModels") return labels.autoNoModels;
  if (failure?.type === "AllFailed") {
    const tried = failure.data.attempts.map((attempt) => `${fileName(attempt.program)} + ${fileName(attempt.model)}: ${attempt.reason}`);
    return [labels.autoAllFailed.replace("{n}", String(failure.data.attempts.length)), ...tried, failure.data.reason].join("\n");
  }
  return describe(error);
}

/**
 * The configured engines and the main one (P2-11), with Java's entries for them: the engine menu,
 * 设置 → 引擎, 引擎规则, restart and shut down, and importing the Java app's configuration. A
 * first run with no engine picks one by itself and opens setup only when that fails.
 */
export function useEngines(
  backend: EnginesBackend,
  status: EngineStatus,
  /** Names for the catalog models Rust identifies, in the interface language. */
  catalog: Readonly<Record<string, string>>,
  labels: EngineLabels,
  setupLabels: SetupLabels & { autoNoPrograms: string; autoNoModels: string; autoAllFailed: string },
  /** Settings changed wholesale by an import; the app reloads what it keeps itself. */
  onImported: (settings: Settings) => void,
) {
  const [engines, setEngines] = useState<Engine[]>([]);
  const [main, setMain] = useState<string | null>(null);
  const [options, setOptions] = useState<EngineOptions>(ENGINE_DEFAULTS);
  const [autoSetup, setAutoSetup] = useState(false);
  const [autoFailure, setAutoFailure] = useState<string | null>(null);
  const [setup, setSetup] = useState<Setup | null>(null);
  const [listOpen, setListOpen] = useState(false);
  const [rulesOpen, setRulesOpen] = useState(false);
  const [preview, setPreview] = useState<JavaImportPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const apply = useCallback((settings: Settings) => {
    const state = engineState(settings);
    setEngines(state.engines);
    setMain(state.main);
    setOptions(state.options);
  }, []);

  /** Reads the saved engines; with none, picks one as the first run does. */
  const load = useCallback(
    async (settings: Settings, active: () => boolean) => {
      apply(settings);
      if (settings.main_engine_id || settings.engine) return;
      setAutoSetup(true);
      try {
        await backend.autoConfigureEngine();
        if (active()) apply(await backend.settings());
      } catch (error) {
        if (!active()) return;
        setAutoFailure(autoSetupFailure(error, setupLabels));
        setSetup({ kind: "main" });
      } finally {
        if (active()) setAutoSetup(false);
      }
    },
    [apply, backend, setupLabels],
  );

  /** A command that returns the new settings; a failure is shown and changes nothing. */
  const run = (action: () => Promise<Settings>) => {
    setBusy(true);
    action()
      .then(apply)
      .catch((error: unknown) => setFailure(describe(error)))
      .finally(() => setBusy(false));
  };

  const mainEngine = engines.find((engine) => engine.id === main) ?? null;
  // The main engine's name as Rust gives it; null until Rust has answered.
  const mainLabel = useEngineName(backend, mainEngine?.profile ?? null, catalog);
  const nameOf = (engine: Engine) => engineName(engine, engine.id === main, mainLabel);
  const running = status.type === "Starting" || status.type === "Ready" || status.type === "Restarting";

  const move = (id: string, by: -1 | 1) => {
    const ids = engines.map((engine) => engine.id);
    const from = ids.indexOf(id);
    const to = from + by;
    if (from < 0 || to < 0 || to >= ids.length) return;
    [ids[from], ids[to]] = [ids[to]!, ids[from]!];
    run(() => backend.reorderEngines(ids));
  };

  const setRules = (rules: EngineRules) => {
    setRulesOpen(false);
    setBusy(true);
    backend
      .setEngineSettings({ ...options, rules })
      .then((saved) => setOptions({ ...ENGINE_DEFAULTS, ...saved }))
      .catch((error: unknown) => setFailure(describe(error)))
      .finally(() => setBusy(false));
  };

  const saveOptions = async (next: EngineOptions) => {
    setOptions({ ...ENGINE_DEFAULTS, ...(await backend.setEngineSettings(next)) });
  };

  const importJava = async () => {
    const directory = await backend.pickDirectory();
    if (!directory) return;
    setBusy(true);
    try {
      setPreview(await backend.previewJavaImport(directory));
    } catch (error) {
      setFailure(describe(error));
    } finally {
      setBusy(false);
    }
  };

  const confirmImport = () => {
    const id = preview?.preview_id;
    if (!id) return;
    setBusy(true);
    backend
      .confirmJavaImport(id)
      .then((settings) => {
        apply(settings);
        onImported(settings);
        setPreview(null);
      })
      .catch((error: unknown) => setFailure(describe(error)))
      .finally(() => setBusy(false));
  };

  const bindings: Record<string, MenuBinding> = {
    engineConfig: { run: () => setListOpen(true) },
    engineRules: { run: () => setRulesOpen(true) },
    importJavaConfig: { run: () => void importJava().catch((error: unknown) => setFailure(describe(error))) },
    restartCurrentEngine: { run: () => run(() => backend.restartEngine()), enabled: mainEngine !== null && !busy },
    shutdownCurrentEngine: { run: () => run(() => backend.closeEngine()), enabled: running && !busy },
  };
  engines.forEach((engine, index) => {
    bindings[`engine${index}`] = {
      run: () => {
        if (engine.id !== main || !running) run(() => backend.selectMainEngine(engine.id));
      },
      enabled: !busy,
      checked: engine.id === main,
    };
  });

  const setupDialog = (): ReactNode => {
    if (!setup) return null;
    const editing = setup.kind === "edit" ? (engines.find((engine) => engine.id === setup.id) ?? null) : null;
    const close = () => {
      setSetup(null);
      setAutoFailure(null);
    };
    const entry =
      setup.kind === "main"
        ? undefined
        : {
            name: editing?.name ?? "",
            nameLabel: labels.nameLabel,
            title: setup.kind === "add" ? labels.addTitle : labels.editTitle,
            save: async (profile: Parameters<Backend["addEngine"]>[1], name: string) => {
              const settings = editing
                ? await backend.updateEngine(editing.id, name, profile)
                : await backend.addEngine(name || nameFromModel(profile.model), profile);
              apply(settings);
            },
          };
    return (
      <EngineSetup
        backend={backend}
        labels={setupLabels}
        current={setup.kind === "main" ? (mainEngine?.profile ?? null) : (editing?.profile ?? null)}
        notice={autoFailure}
        engineRunning={setup.kind === "edit" ? false : running}
        entry={entry}
        onConfigured={() => {
          close();
          if (setup.kind === "main") void backend.settings().then(apply);
        }}
        onClose={close}
      />
    );
  };

  const dialogs = (
    <>
      {listOpen && !setup ? (
        <EngineList
          engines={engines}
          main={main}
          nameOf={nameOf}
          busy={busy}
          labels={labels.list}
          onSetMain={(id) => run(() => backend.selectMainEngine(id))}
          onEdit={(id) => setSetup({ kind: "edit", id })}
          onRemove={(id) => run(() => backend.removeEngine(id))}
          onMove={move}
          onAdd={() => setSetup({ kind: "add" })}
          onClose={() => setListOpen(false)}
        />
      ) : null}
      {setupDialog()}
      {rulesOpen ? <RulesDialog rules={options.rules} labels={labels.rules} onSave={setRules} onClose={() => setRulesOpen(false)} /> : null}
      {preview ? <JavaImportDialog preview={preview} busy={busy} labels={labels.javaImport} onConfirm={confirmImport} onClose={() => setPreview(null)} /> : null}
      {failure ? <MessageBox title={labels.failedTitle} lines={[failure]} ok={labels.ok} testId="engine-failed" onClose={() => setFailure(null)} /> : null}
    </>
  );

  return {
    engines,
    main,
    /** The main engine's profile, or null when none is configured. */
    profile: mainEngine?.profile ?? null,
    /** The main engine's name as Rust gives it: null with no engine, "" until Rust has named it. */
    label: mainEngine ? (mainLabel ?? "") : null,
    mainIndex: Math.max(0, engines.findIndex((engine) => engine.id === main)),
    /** Menu names, in order, and the index of the main engine. */
    menuNames: engines.map(nameOf),
    options,
    saveOptions,
    /** Settings a command elsewhere returned, e.g. choosing an official weight. */
    apply,
    autoSetup,
    load,
    openSetup: () => setSetup(mainEngine ? { kind: "edit", id: mainEngine.id } : { kind: "main" }),
    bindings,
    dialogs,
    dialogOpen: listOpen || setup !== null || rulesOpen || preview !== null || failure !== null,
  };
}
