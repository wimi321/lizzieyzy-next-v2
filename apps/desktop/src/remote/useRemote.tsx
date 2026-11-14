import { useCallback, useEffect, useState } from "react";
import type { Backend, EngineProfile, Settings } from "@/api/backend";
import type { RemoteEngineInput } from "../api/remote";
import type { ZhiziPlan, ZhiziStatus, ZhiziWeight } from "../api/zhizi";
import type { Engine } from "../engines/model";
import type en from "../locales/en.json";
import type { MenuBindings } from "../menu/bindings";
import { remoteError, remoteKind, remoteSummary } from "./format";
import { SelfHosted } from "./SelfHosted";
import { ZhiziAccount } from "./ZhiziAccount";
import { ZhiziLogin } from "./ZhiziLogin";

type RemoteBackend = Pick<
  Backend,
  | "zhiziStatus"
  | "zhiziLoginPassword"
  | "zhiziLoginCode"
  | "zhiziSendCode"
  | "zhiziResetPassword"
  | "zhiziLogout"
  | "zhiziOverview"
  | "zhiziUsages"
  | "zhiziCredits"
  | "normalizeRemoteLink"
  | "readRemoteLinkQr"
  | "testRemoteEngine"
  | "addRemoteEngine"
  | "updateRemoteEngine"
  | "switchToLocalEngine"
  | "pickQrImage"
  | "addEngine"
  | "updateEngine"
  | "selectMainEngine"
  | "openZhiziWebsite"
  | "zhiziWebsite"
>;

export interface RemoteEngines {
  engines: readonly Engine[];
  main: string | null;
  profile: EngineProfile | null;
  apply(settings: Settings): void;
}

type Tab = "zhizi" | "self";

function report(error: unknown) {
  console.error("Remote compute command failed", error);
}

/** The engine of this kind that `settings` has and the list before did not. */
function added(settings: Settings, before: readonly Engine[], kind: "Zhizi" | "WebSocket"): string | null {
  const known = new Set(before.map((engine) => engine.id));
  const fresh = (settings.engines ?? []).find((entry) => entry.id !== undefined && !known.has(entry.id) && entry.profile?.remote?.type === kind);
  return fresh?.id ?? null;
}

/**
 * P3-20: 设置 → 远程算力. Rust connects, keeps the secrets and runs the remote engines; this logs in
 * to 智子, picks a plan or a self-hosted link, and makes it the main engine.
 */
export function useRemote(backend: RemoteBackend, labels: typeof en.remote, engines: RemoteEngines) {
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<Tab>("zhizi");
  const [zhizi, setZhizi] = useState<ZhiziStatus | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const kind = remoteKind(engines.profile);
  const main = engines.engines.find((engine) => engine.id === engines.main) ?? null;

  useEffect(() => {
    if (!open || !backend.zhiziStatus) return;
    let active = true;
    backend
      .zhiziStatus()
      .then((status) => active && setZhizi(status))
      .catch(report);
    return () => {
      active = false;
    };
  }, [open, backend]);

  const show = useCallback((which?: Tab) => {
    setFailure(null);
    if (which) setTab(which);
    setOpen(true);
  }, []);

  const select = async (settings: Settings, id: string | null) => {
    const next = id && settings.main_engine_id !== id ? await backend.selectMainEngine(id) : settings;
    engines.apply(next);
  };
  const connectZhizi = async (plan: ZhiziPlan, weight: ZhiziWeight) => {
    const profile: EngineProfile = { program: "", model: "", config: "", extra_args: [], remote: { type: "Zhizi", data: { options: { plan, weight } } } };
    const existing = engines.engines.find((engine) => engine.profile.remote?.type === "Zhizi");
    if (existing) return select(await backend.updateEngine(existing.id, existing.name, profile), existing.id);
    const settings = await backend.addEngine(labels.zhizi.engineName, profile);
    await select(settings, added(settings, engines.engines, "Zhizi"));
  };
  const enableSelf = async (url: string) => {
    const entry: RemoteEngineInput = { type: "WebSocket", data: { url } };
    if (main && kind === "WebSocket") return select(await backend.updateRemoteEngine!(main.id, main.name, entry), main.id);
    const settings = await backend.addRemoteEngine!(labels.self.engineName, entry);
    await select(settings, added(settings, engines.engines, "WebSocket"));
  };
  // 充值 and 开通 VIP: Rust opens the fixed official site; without a browser, the address is shown.
  const openWebsite = () => {
    setFailure(null);
    backend
      .openZhiziWebsite?.()
      .catch(() => backend.zhiziWebsite?.().then((url) => setFailure(labels.browserFailed.replace("{url}", url))))
      .catch(report);
  };
  const toLocal = () => {
    setFailure(null);
    backend
      .switchToLocalEngine?.()
      .then(engines.apply)
      .catch((error: unknown) => setFailure(remoteError(error, labels)));
  };

  const zhiziOptions = engines.engines.find((engine) => engine.profile.remote?.type === "Zhizi")?.profile.remote;
  const options = zhiziOptions?.type === "Zhizi" ? zhiziOptions.data.options : undefined;
  const current = kind ? labels.current[kind] : engines.profile ? labels.current.local : labels.current.none;
  const selfAddress = engines.profile?.remote?.type === "WebSocket" ? `${engines.profile.remote.data.host ?? ""}:${engines.profile.remote.data.port ?? ""}` : null;

  const dialog = open ? (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog remote-dialog" role="dialog" aria-modal="true" aria-labelledby="remote-title" data-testid="remote-dialog">
        <header className="dialog-header">
          <h2 id="remote-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} data-testid="remote-close" onClick={() => setOpen(false)}>
            ×
          </button>
        </header>
        <div className="notes-tabs remote-tabs" role="tablist">
          {(["zhizi", "self"] as const).map((value) => (
            <button key={value} type="button" role="tab" aria-selected={tab === value} data-testid={`remote-tab-${value}`} onClick={() => setTab(value)}>
              {value === "zhizi" ? labels.zhizi.title : labels.self.title}
            </button>
          ))}
        </div>
        <div className="remote-body">
          {tab === "self" ? (
            <SelfHosted backend={backend} labels={labels} current={selfAddress} onEnable={enableSelf} />
          ) : zhizi === null ? (
            <p className="panel-empty">{labels.zhizi.loading}</p>
          ) : zhizi.logged_in ? (
            <ZhiziAccount
              backend={backend}
              labels={labels}
              plan={options?.plan ?? "Vip"}
              weight={options?.weight ?? "10b512t"}
              active={kind === "Zhizi"}
              onConnect={connectZhizi}
              onWebsite={openWebsite}
              onLoggedOut={setZhizi}
            />
          ) : (
            <ZhiziLogin backend={backend} labels={labels} status={zhizi} onLoggedIn={setZhizi} />
          )}
        </div>
        <footer className="remote-footer">
          <span data-testid="remote-current">{current}</span>
          {failure ? (
            <span className="remote-failure" role="alert">
              {failure}
            </span>
          ) : null}
          <button type="button" className="secondary-button" data-testid="remote-local" disabled={kind === null} onClick={toLocal}>
            {labels.backToLocal}
          </button>
        </footer>
      </section>
    </div>
  ) : null;

  const bindings: MenuBindings = { remoteCompute: { run: () => show() } };

  return {
    bindings,
    dialog,
    dialogOpen: open,
    open: show,
    /** The main engine is remote: the benchmark, auto setup, thread advice and the coach don't apply. */
    remote: kind !== null,
    /** The engine's name with where it runs, for the menu strip and the status line. */
    label: (name: string | null) => {
      if (!kind || name === null) return name;
      // A remote profile has no model file to name it by; the entry's own name stands in.
      const base = name || main?.name || "";
      const where = labels.kinds[kind];
      return base.includes(where) ? base : base ? `${base} (${where})` : where;
    },
    summary: (engine: Engine) => remoteSummary(engine.profile, labels),
  };
}
