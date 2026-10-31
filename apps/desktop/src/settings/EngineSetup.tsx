import { useEffect, useState } from "react";
import type { Backend, EngineDiscovery, EngineFileKind, EngineProfile, EngineSetupProfile } from "@/api/backend";

export interface SetupLabels {
  title: string;
  titleCurrent: string;
  intro: string;
  introCurrent: string;
  program: string;
  model: string;
  config: string;
  defaultConfig: string;
  browse: string;
  recommended: string;
  notFound: string;
  detecting: string;
  start: string;
  apply: string;
  starting: string;
  close: string;
  failed: string;
  autoFailed: string;
}

type SetupBackend = Pick<Backend, "detectEngines" | "tryConfigureEngine" | "pickEngineFile">;

export interface EngineSetupProps {
  backend: SetupBackend;
  labels: SetupLabels;
  /** The saved profile, preselected when the panel is reopened. */
  current: EngineProfile | null;
  /** The saved profile's engine is starting or ready, so applying it unchanged would do nothing. */
  engineRunning: boolean;
  /** Why automatic setup gave up, when it opened this dialog. */
  notice?: string | null;
  /** Called after the engine reached Ready and the profile was saved. */
  onConfigured(profile: EngineSetupProfile): void;
  onClose(): void;
}

type Phase = { kind: "editing" } | { kind: "starting" } | { kind: "failed"; message: string };

/** Picks KataGo files and starts the engine before anything is saved (try-then-save). */
export function EngineSetup({ backend, labels, current, engineRunning, notice = null, onConfigured, onClose }: EngineSetupProps) {
  const [discovery, setDiscovery] = useState<EngineDiscovery | null>(null);
  const [program, setProgram] = useState(current?.program ?? "");
  const [model, setModel] = useState(current?.model ?? "");
  const [config, setConfig] = useState(current?.config ?? "");
  const [extras, setExtras] = useState<Record<EngineFileKind, string[]>>({ program: [], model: [], config: [] });
  const [phase, setPhase] = useState<Phase>({ kind: "editing" });

  useEffect(() => {
    let active = true;
    backend
      .detectEngines()
      .then((found) => {
        if (!active) return;
        setDiscovery(found);
        // Discovery lists the recommended file first; keep any saved choice.
        setProgram((value) => value || (found.programs[0] ?? ""));
        setModel((value) => value || (found.models[0] ?? ""));
      })
      .catch((error: unknown) => {
        if (active) setDiscovery({ programs: [], models: [], configs: [] });
        console.error("Engine discovery failed", error);
      });
    return () => {
      active = false;
    };
  }, [backend]);

  const browse = async (kind: EngineFileKind) => {
    const path = await backend.pickEngineFile(kind);
    if (!path) return;
    setExtras((previous) => ({ ...previous, [kind]: [...previous[kind].filter((p) => p !== path), path] }));
    if (kind === "program") setProgram(path);
    if (kind === "model") setModel(path);
    if (kind === "config") setConfig(path);
  };

  const start = async () => {
    setPhase({ kind: "starting" });
    const profile: EngineSetupProfile = { program, model, config: config || null, extra_args: current?.extra_args ?? [] };
    try {
      await backend.tryConfigureEngine(profile);
      onConfigured(profile);
    } catch (error) {
      setPhase({ kind: "failed", message: describe(error) });
    }
  };

  const options = (kind: EngineFileKind, found: string[], selected: string) =>
    unique([...found, ...extras[kind], ...(selected ? [selected] : [])]);
  const busy = phase.kind === "starting";
  const nothingFound = discovery !== null && discovery.programs.length === 0 && discovery.models.length === 0;
  const unchanged = current !== null && program === current.program && model === current.model && config === current.config;
  const intro = current ? labels.introCurrent : discovery === null ? labels.detecting : nothingFound ? labels.notFound : labels.intro;

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog" role="dialog" aria-modal="true" aria-labelledby="engine-setup-title" data-testid="engine-setup">
        <header className="dialog-header">
          <h2 id="engine-setup-title">{current ? labels.titleCurrent : labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} data-testid="setup-close" onClick={onClose} disabled={busy}>
            ×
          </button>
        </header>
        <p className="dialog-intro">{intro}</p>
        <FileField
          kind="program"
          label={labels.program}
          value={program}
          options={options("program", discovery?.programs ?? [], program)}
          recommended={discovery?.programs[0]}
          labels={labels}
          disabled={busy}
          onChange={setProgram}
          onBrowse={() => void browse("program")}
        />
        <FileField
          kind="model"
          label={labels.model}
          value={model}
          options={options("model", discovery?.models ?? [], model)}
          recommended={discovery?.models[0]}
          labels={labels}
          disabled={busy}
          onChange={setModel}
          onBrowse={() => void browse("model")}
        />
        <FileField
          kind="config"
          label={labels.config}
          value={config}
          options={options("config", discovery?.configs ?? [], config)}
          labels={labels}
          disabled={busy}
          emptyOption={labels.defaultConfig}
          onChange={setConfig}
          onBrowse={() => void browse("config")}
        />
        {notice && phase.kind === "editing" ? (
          <div className="dialog-error" role="alert" data-testid="setup-notice">
            <strong>{labels.autoFailed}</strong>
            <pre>{notice}</pre>
          </div>
        ) : null}
        {phase.kind === "failed" ? (
          <div className="dialog-error" role="alert" data-testid="setup-error">
            <strong>{labels.failed}</strong>
            <pre>{phase.message}</pre>
          </div>
        ) : null}
        <footer className="dialog-footer">
          <button
            type="button"
            className="primary-button"
            data-testid="setup-start"
            disabled={busy || !program || !model || (unchanged && engineRunning)}
            onClick={() => void start()}
          >
            {busy ? labels.starting : current ? labels.apply : labels.start}
          </button>
        </footer>
      </section>
    </div>
  );
}

function FileField(props: {
  kind: EngineFileKind;
  label: string;
  value: string;
  options: string[];
  recommended?: string;
  labels: SetupLabels;
  disabled: boolean;
  emptyOption?: string;
  onChange(value: string): void;
  onBrowse(): void;
}) {
  const id = `setup-${props.kind}`;
  return (
    <div className="field">
      <label htmlFor={id}>{props.label}</label>
      <div className="field-row">
        <select
          id={id}
          data-testid={id}
          value={props.value}
          disabled={props.disabled}
          onChange={(event) => props.onChange(event.target.value)}
        >
          {props.emptyOption !== undefined ? <option value="">{props.emptyOption}</option> : null}
          {props.emptyOption === undefined && !props.value ? <option value="" /> : null}
          {props.options.map((path) => (
            <option key={path} value={path} title={path}>
              {fileName(path)}
              {path === props.recommended ? ` · ${props.labels.recommended}` : ""}
            </option>
          ))}
        </select>
        <button type="button" className="secondary-button" data-testid={`${id}-browse`} disabled={props.disabled} onClick={props.onBrowse}>
          {props.labels.browse}
        </button>
      </div>
      {props.value ? <p className="field-path" title={props.value}>{props.value}</p> : null}
    </div>
  );
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function unique(paths: string[]): string[] {
  return [...new Set(paths)];
}

/** Command errors arrive as `{ type: "App", data: message }`; anything else is shown as text. */
function describe(error: unknown): string {
  if (typeof error === "object" && error !== null && "data" in error && typeof error.data === "string") return error.data;
  if (error instanceof Error) return error.message;
  return String(error);
}
