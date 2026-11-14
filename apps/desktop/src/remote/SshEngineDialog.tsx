import { type FormEvent, useEffect, useState } from "react";
import type { Backend, Settings } from "@/api/backend";
import type { RemoteEngineInput } from "../api/remote";
import type { Engine } from "../engines/model";
import type en from "../locales/en.json";
import { changedHostKey, remoteError } from "./format";

type SshBackend = Pick<Backend, "testRemoteEngine" | "addRemoteEngine" | "updateRemoteEngine" | "pickKeyFile">;

export interface SshEngineDialogProps {
  backend: SshBackend;
  labels: typeof en.remote;
  /** The SSH engine being edited, or null for a new one. */
  editing: Engine | null;
  onSaved(settings: Settings): void;
  onClose(): void;
}

type HostKey = NonNullable<ReturnType<typeof changedHostKey>>;

/** 设置 → 引擎 → SSH 远程引擎: Rust's built-in client runs the command on the server. */
export function SshEngineDialog({ backend, labels, editing, onSaved, onClose }: SshEngineDialogProps) {
  const text = labels.ssh;
  const saved = editing?.profile.remote?.type === "Ssh" ? editing.profile.remote.data : null;
  const [name, setName] = useState(editing?.name ?? "");
  const [host, setHost] = useState(saved?.host ?? "");
  const [port, setPort] = useState(String(saved?.port ?? 22));
  const [user, setUser] = useState(saved?.user ?? "");
  const [command, setCommand] = useState(saved?.command ?? "");
  const [method, setMethod] = useState<"Password" | "PrivateKey">(saved?.auth?.type ?? "Password");
  const [password, setPassword] = useState("");
  const [keyPath, setKeyPath] = useState(saved?.auth?.type === "PrivateKey" ? (saved.auth.data.path ?? "") : "");
  const [passphrase, setPassphrase] = useState("");
  const [busy, setBusy] = useState(false);
  const [version, setVersion] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hostKey, setHostKey] = useState<HostKey | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const input = (): RemoteEngineInput | string => {
    const number = Number(port);
    if (!host.trim() || !user.trim() || !command.trim()) return text.required;
    if (!Number.isInteger(number) || number < 1 || number > 65535) return text.invalidPort;
    if (method === "Password" && !password) return text.passwordRequired;
    if (method === "PrivateKey" && !keyPath.trim()) return text.keyRequired;
    const auth = method === "Password" ? { type: "Password" as const, data: { password } } : { type: "PrivateKey" as const, data: { path: keyPath.trim(), passphrase: passphrase || null } };
    return { type: "Ssh", data: { host: host.trim(), port: number, user: user.trim(), command: command.trim(), auth } };
  };
  const run = (work: (entry: RemoteEngineInput) => Promise<void>) => {
    const entry = input();
    if (typeof entry === "string") return setError(entry);
    setBusy(true);
    setError(null);
    setHostKey(null);
    setVersion(null);
    work(entry)
      .catch((failure: unknown) => {
        setHostKey(changedHostKey(failure));
        setError(remoteError(failure, labels));
      })
      .finally(() => setBusy(false));
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    run(async (entry) => {
      const title = name.trim() || `${user.trim()}@${host.trim()}`;
      onSaved(editing ? await backend.updateRemoteEngine!(editing.id, title, entry) : await backend.addRemoteEngine!(title, entry));
    });
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog ssh-dialog" role="dialog" aria-modal="true" aria-labelledby="ssh-title" data-testid="ssh-dialog">
        <header className="dialog-header">
          <h2 id="ssh-title">{editing ? text.editTitle : text.addTitle}</h2>
          <button type="button" className="dialog-close" aria-label={text.cancel} onClick={onClose}>
            ×
          </button>
        </header>
        <form className="ssh-form" noValidate onSubmit={submit}>
          <div className="remote-grid">
            <label htmlFor="ssh-name">{text.name}</label>
            <input id="ssh-name" className="text-input" data-testid="ssh-name" value={name} placeholder={text.namePlaceholder} onChange={(event) => setName(event.target.value)} />
            <label htmlFor="ssh-host">{text.host}</label>
            <span className="remote-row">
              <input id="ssh-host" className="text-input" data-testid="ssh-host" spellCheck={false} value={host} onChange={(event) => setHost(event.target.value)} />
              <span className="ssh-port-label">{text.port}</span>
              <input className="text-input ssh-port" data-testid="ssh-port" inputMode="numeric" aria-label={text.port} value={port} onChange={(event) => setPort(event.target.value)} />
            </span>
            <label htmlFor="ssh-user">{text.user}</label>
            <input id="ssh-user" className="text-input" data-testid="ssh-user" spellCheck={false} autoComplete="off" value={user} onChange={(event) => setUser(event.target.value)} />
            <span>{text.auth}</span>
            <div className="segmented small" role="radiogroup" aria-label={text.auth}>
              {(["Password", "PrivateKey"] as const).map((value) => (
                <button key={value} type="button" role="radio" aria-checked={method === value} data-testid={`ssh-auth-${value}`} onClick={() => setMethod(value)}>
                  {value === "Password" ? text.password : text.privateKey}
                </button>
              ))}
            </div>
            {method === "Password" ? (
              <>
                <label htmlFor="ssh-password">{text.password}</label>
                <input
                  id="ssh-password"
                  className="text-input"
                  data-testid="ssh-password"
                  type="password"
                  autoComplete="off"
                  placeholder={editing ? text.passwordAgain : ""}
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                />
              </>
            ) : (
              <>
                <label htmlFor="ssh-key">{text.keyFile}</label>
                <span className="remote-row">
                  <input id="ssh-key" className="text-input" data-testid="ssh-key" spellCheck={false} value={keyPath} onChange={(event) => setKeyPath(event.target.value)} />
                  <button type="button" className="secondary-button" data-testid="ssh-browse" onClick={() => void backend.pickKeyFile().then((path) => path && setKeyPath(path))}>
                    {text.browse}
                  </button>
                </span>
                <label htmlFor="ssh-passphrase">{text.passphrase}</label>
                <input id="ssh-passphrase" className="text-input" data-testid="ssh-passphrase" type="password" autoComplete="off" placeholder={text.passphraseHint} value={passphrase} onChange={(event) => setPassphrase(event.target.value)} />
              </>
            )}
            <label htmlFor="ssh-command">{text.command}</label>
            <input id="ssh-command" className="text-input" data-testid="ssh-command" spellCheck={false} placeholder={text.commandPlaceholder} value={command} onChange={(event) => setCommand(event.target.value)} />
          </div>
          <p className="remote-hint">{text.secretNote}</p>
          {version ? (
            <p className="remote-note" data-testid="ssh-version">
              {text.version.replace("{version}", version)}
            </p>
          ) : null}
          {hostKey ? (
            <div className="ssh-host-key" role="alert" data-testid="ssh-host-key">
              <strong>{text.hostKeyTitle}</strong>
              <p>{text.hostKeyBody.replace("{host}", host.trim())}</p>
              <dl>
                <dt>{text.recorded}</dt>
                <dd>{hostKey.old_fingerprint}</dd>
                <dt>{text.presented}</dt>
                <dd>{hostKey.new_fingerprint}</dd>
                <dt>{text.knownHosts}</dt>
                <dd>{hostKey.path}</dd>
              </dl>
            </div>
          ) : error ? (
            <p className="dialog-error" role="alert" data-testid="ssh-error">
              {error}
            </p>
          ) : null}
          <footer className="dialog-footer">
            <button type="button" className="secondary-button" data-testid="ssh-test" disabled={busy} onClick={() => run(async (entry) => setVersion(await backend.testRemoteEngine!(entry)))}>
              {text.test}
            </button>
            <button type="button" className="secondary-button" onClick={onClose}>
              {text.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="ssh-save" disabled={busy}>
              {text.save}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
