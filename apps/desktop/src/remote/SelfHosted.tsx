import { useState } from "react";
import type { Backend } from "@/api/backend";
import type en from "../locales/en.json";
import { remoteError } from "./format";

type SelfHostedBackend = Pick<Backend, "normalizeRemoteLink" | "readRemoteLinkQr" | "testRemoteEngine" | "pickQrImage">;

export interface SelfHostedProps {
  backend: SelfHostedBackend;
  labels: typeof en.remote;
  /** Where the main engine connects now, when it is a self-hosted one ("host:port"). */
  current: string | null;
  onEnable(url: string): Promise<void>;
}

/** 自建算力: a KaTrain-compatible ws/wss link, pasted or read from a QR code, tested, then used. */
export function SelfHosted({ backend, labels, current, onEnable }: SelfHostedProps) {
  const text = labels.self;
  const [link, setLink] = useState("");
  const [version, setVersion] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    work()
      .catch((failure: unknown) => setError(remoteError(failure, labels)))
      .finally(() => setBusy(false));
  };
  // Rust tidies what was pasted: full-width letters, quotes, text around the link.
  const normalize = (value: string) => {
    if (!value.trim() || !backend.normalizeRemoteLink) return;
    run(async () => setLink(await backend.normalizeRemoteLink!(value)));
  };
  const importQr = () =>
    run(async () => {
      const path = await backend.pickQrImage();
      if (path) setLink(await backend.readRemoteLinkQr!(path));
    });
  const url = link.trim();
  const valid = /^wss?:\/\/\S+$/i.test(url);

  return (
    <div className="remote-self" data-testid="remote-self">
      <p className="remote-hint">{text.description}</p>
      <label className="remote-field">
        <span>{text.link}</span>
        <span className="remote-row">
          <input
            className="text-input"
            data-testid="remote-link"
            spellCheck={false}
            placeholder={text.placeholder}
            value={link}
            onChange={(event) => {
              setLink(event.target.value);
              setVersion(null);
            }}
            onPaste={(event) => {
              event.preventDefault();
              normalize(event.clipboardData.getData("text"));
            }}
            onBlur={() => url && !valid && normalize(url)}
          />
          <button type="button" className="secondary-button" data-testid="remote-qr" disabled={busy} onClick={importQr}>
            {text.importQr}
          </button>
        </span>
      </label>
      {/^ws:\/\//i.test(url) ? (
        <p className="remote-warning" data-testid="remote-insecure">
          {text.insecure}
        </p>
      ) : null}
      {current ? <p className="remote-note">{text.current.replace("{address}", current)}</p> : null}
      {version ? (
        <p className="remote-note" data-testid="remote-version">
          {text.version.replace("{version}", version)}
        </p>
      ) : null}
      {error ? (
        <p className="dialog-error" role="alert" data-testid="remote-error">
          {error}
        </p>
      ) : null}
      <div className="remote-actions">
        <button
          type="button"
          className="secondary-button"
          data-testid="remote-test"
          disabled={busy || !valid}
          onClick={() => run(async () => setVersion(await backend.testRemoteEngine!({ type: "WebSocket", data: { url } })))}
        >
          {text.test}
        </button>
        <button type="button" className="primary-button" data-testid="remote-enable" disabled={busy || !valid} onClick={() => run(() => onEnable(url))}>
          {current ? text.change : text.enable}
        </button>
      </div>
    </div>
  );
}
