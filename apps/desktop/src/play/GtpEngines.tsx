import { type FormEvent, useEffect, useRef, useState } from "react";
import type { GtpConsoleLine, GtpEngineEntry } from "../api/gtp";
import type { GtpLabels } from "./labels";
import { PlayWindow } from "./PlayDialog";
import { joinArgs, splitArgs } from "./setup";

interface GtpEnginesProps {
  engines: readonly GtpEngineEntry[];
  labels: GtpLabels;
  busy: boolean;
  /** Resolves with the reason the save failed, or null once saved. */
  onSave(entry: GtpEngineEntry): Promise<string | null>;
  onRemove(id: string): void;
  /** Resolves with the line to show: what the engine said its name is, or why it failed. */
  onTest(id: string): Promise<{ ok: boolean; text: string }>;
  pickProgram(): Promise<string | null>;
  pickFolder(): Promise<string | null>;
  onClose(): void;
}

interface Draft {
  id: string;
  name: string;
  program: string;
  args: string;
  workingDir: string;
  init: string;
}

const draftOf = (entry?: GtpEngineEntry): Draft => ({
  id: entry?.id ?? "",
  name: entry?.name ?? "",
  program: entry?.program ?? "",
  args: joinArgs(entry?.args ?? []),
  workingDir: entry?.working_dir ?? "",
  init: (entry?.init_commands ?? []).join("\n"),
});

/** 设置 → GTP 引擎…: third-party engines for games and engine matches, apart from KataGo's list. */
export function GtpEngines({ engines, labels, busy, onSave, onRemove, onTest, pickProgram, pickFolder, onClose }: GtpEnginesProps) {
  const [draft, setDraft] = useState<Draft | null>(null);
  const [tests, setTests] = useState<Record<string, { ok: boolean; text: string } | "testing">>({});
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<Draft>) => setDraft((current) => (current ? { ...current, ...patch } : current));
  const ready = draft !== null && draft.name.trim() !== "" && draft.program.trim() !== "" && !busy;

  const save = (event: FormEvent) => {
    event.preventDefault();
    if (!draft || !ready) return;
    const entry: GtpEngineEntry = {
      id: draft.id,
      name: draft.name.trim(),
      program: draft.program.trim(),
      args: splitArgs(draft.args),
      working_dir: draft.workingDir.trim() || null,
      init_commands: draft.init
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => line.length > 0),
    };
    void onSave(entry).then((failure) => {
      if (failure) setError(failure);
      else {
        setError(null);
        setDraft(null);
      }
    });
  };

  const test = (id: string) => {
    setTests((current) => ({ ...current, [id]: "testing" }));
    void onTest(id).then((result) => setTests((current) => ({ ...current, [id]: result })));
  };

  if (draft) {
    return (
      <PlayWindow title={draft.id ? `${labels.edit} · ${draft.name}` : labels.add} testId="gtp-editor" onClose={() => setDraft(null)} close={labels.close}>
        <form className="play-form" noValidate onSubmit={save}>
          <div className="play-panel">
            <label className="play-row">
              <span className="play-label">{labels.name}</span>
              <input className="text-input play-text" value={draft.name} aria-invalid={draft.name.trim() === ""} data-testid="gtp-name" onChange={(event) => set({ name: event.target.value })} />
            </label>
            <div className="play-row">
              <span className="play-label">{labels.program}</span>
              <span className="play-inline play-grow">
                <input className="text-input play-text" value={draft.program} aria-invalid={draft.program.trim() === ""} data-testid="gtp-program" onChange={(event) => set({ program: event.target.value })} />
                <button type="button" className="secondary-button" onClick={() => void pickProgram().then((path) => path && set({ program: path }))}>
                  {labels.browse}
                </button>
              </span>
            </div>
            <label className="play-row">
              <span className="play-label">{labels.args}</span>
              <input className="text-input play-text mono" value={draft.args} data-testid="gtp-args" onChange={(event) => set({ args: event.target.value })} />
            </label>
            <p className="settings-hint play-indent-label">{labels.argsHint}</p>
            <div className="play-row">
              <span className="play-label">{labels.workingDir}</span>
              <span className="play-inline play-grow">
                <input className="text-input play-text" value={draft.workingDir} placeholder={labels.optional} data-testid="gtp-dir" onChange={(event) => set({ workingDir: event.target.value })} />
                <button type="button" className="secondary-button" onClick={() => void pickFolder().then((path) => path && set({ workingDir: path }))}>
                  {labels.browse}
                </button>
              </span>
            </div>
            <label className="play-row play-top">
              <span className="play-label">{labels.initCommands}</span>
              <textarea className="text-input play-commands mono" rows={3} value={draft.init} placeholder={labels.optional} data-testid="gtp-init" onChange={(event) => set({ init: event.target.value })} />
            </label>
            <p className="settings-hint play-indent-label">{labels.initHint}</p>
          </div>
          <footer className="dialog-footer settings-footer">
            {error ? (
              <p className="play-error" role="alert" title={error}>
                {labels.failed.replace("{reason}", error)}
              </p>
            ) : null}
            <button type="button" className="secondary-button" onClick={() => setDraft(null)}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="gtp-save" disabled={!ready}>
              {labels.save}
            </button>
          </footer>
        </form>
      </PlayWindow>
    );
  }

  return (
    <PlayWindow title={labels.title} testId="gtp-engines" onClose={onClose} close={labels.close}>
      <p className="settings-hint gtp-hint">{labels.hint}</p>
      {engines.length === 0 ? (
        <p className="gtp-empty" data-testid="gtp-empty">
          {labels.empty}
        </p>
      ) : (
        <ul className="gtp-list">
          {engines.map((entry) => {
            const id = entry.id ?? "";
            const result = tests[id];
            return (
              <li key={id} className="gtp-row" data-testid="gtp-row">
                <div className="gtp-main">
                  <strong>{entry.name}</strong>
                  <code title={[entry.program, ...(entry.args ?? [])].join(" ")}>{[entry.program, joinArgs(entry.args ?? [])].filter(Boolean).join(" ")}</code>
                  {result ? (
                    <span className={result === "testing" ? "gtp-test" : result.ok ? "gtp-test ok" : "gtp-test failed"} data-testid="gtp-test-result">
                      {result === "testing" ? labels.testing : result.text}
                    </span>
                  ) : null}
                </div>
                <div className="gtp-actions">
                  <button type="button" className="secondary-button" disabled={busy || result === "testing"} data-testid="gtp-test" onClick={() => test(id)}>
                    {labels.test}
                  </button>
                  <button type="button" className="secondary-button" disabled={busy} data-testid="gtp-edit" onClick={() => setDraft(draftOf(entry))}>
                    {labels.edit}
                  </button>
                  <button type="button" className="danger-link" disabled={busy} data-testid="gtp-remove" onClick={() => onRemove(id)}>
                    {labels.remove}
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
      <footer className="dialog-footer settings-footer">
        <button type="button" className="secondary-button" onClick={onClose}>
          {labels.close}
        </button>
        <button type="button" className="primary-button" disabled={busy} data-testid="gtp-add" onClick={() => setDraft(draftOf())}>
          {labels.add}
        </button>
      </footer>
    </PlayWindow>
  );
}

interface GtpConsoleProps {
  engines: readonly GtpEngineEntry[];
  engineId: string | null;
  lines: readonly GtpConsoleLine[];
  labels: GtpLabels;
  onEngine(id: string): void;
  /** Resolves with the engine's reply, or the reason it was refused. */
  onSend(text: string): Promise<{ ok: boolean; text: string }>;
  onClear(): void;
  onClose(): void;
}

/** 显示 → 面板 → Gtp控制台: the chosen GTP engine's transcript, and a line to type commands into. */
export function GtpConsole({ engines, engineId, lines, labels, onEngine, onSend, onClear, onClose }: GtpConsoleProps) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const log = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = log.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [lines]);
  const send = (command: string) => {
    if (!command.trim() || !engineId) return;
    void onSend(command.trim()).then((result) => {
      setError(result.ok ? null : result.text);
      if (result.ok) setText("");
    });
  };
  return (
    <aside className="gtp-console" role="dialog" aria-labelledby="gtp-console-title" data-testid="gtp-console">
      <header className="match-panel-header">
        <h2 id="gtp-console-title">{labels.console}</h2>
        <select className="select compact" value={engineId ?? ""} disabled={engines.length === 0} data-testid="gtp-console-engine" onChange={(event) => onEngine(event.target.value)}>
          {engines.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.name}
            </option>
          ))}
        </select>
        <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
          ×
        </button>
      </header>
      <div className="gtp-log" ref={log} data-testid="gtp-log">
        {engines.length === 0 ? <p className="gtp-empty">{labels.noEngines}</p> : null}
        {lines.map((line) => (
          <div key={line.sequence} className="gtp-line" data-direction={line.direction} data-error={(line.direction === "Received" && line.text?.startsWith("?")) || undefined}>
            {line.direction === "Sent" ? <span className="gtp-prompt">{`${engines.find((entry) => entry.id === line.engine_id)?.name ?? line.engine_id}> `}</span> : null}
            {line.text}
          </div>
        ))}
      </div>
      {error ? (
        <p className="play-error gtp-console-error" role="alert" data-testid="gtp-console-error">
          {error}
        </p>
      ) : null}
      <form
        className="gtp-input"
        onSubmit={(event) => {
          event.preventDefault();
          send(text);
        }}
      >
        <input className="text-input mono" value={text} placeholder={labels.placeholder} disabled={!engineId} data-testid="gtp-console-input" onChange={(event) => setText(event.target.value)} />
        <button type="submit" className="primary-button" disabled={!engineId || !text.trim()} data-testid="gtp-console-send">
          {labels.send}
        </button>
        <button type="button" className="secondary-button" disabled={!engineId} onClick={() => send("list_commands")}>
          {labels.commands}
        </button>
        <button type="button" className="secondary-button" onClick={onClear}>
          {labels.clear}
        </button>
      </form>
    </aside>
  );
}
