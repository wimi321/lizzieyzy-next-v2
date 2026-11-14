import { useCallback, useEffect, useState } from "react";
import type { Backend } from "@/api/backend";
import type { ZhiziCreditPage, ZhiziOverview, ZhiziPlan, ZhiziStatus, ZhiziUsagePage, ZhiziWeight } from "../api/zhizi";
import type en from "../locales/en.json";
import { duration, remoteError, shortTime, yuan } from "./format";

type AccountBackend = Pick<Backend, "zhiziOverview" | "zhiziUsages" | "zhiziCredits" | "zhiziLogout">;

/** Java's plans and weights, in its order (`RemoteComputeConfig`, `ZhiziEngineCatalog`). */
const PLANS: readonly ZhiziPlan[] = ["Vip", "OnDemand1x", "OnDemand3x", "OnDemand6x", "OnDemand12x", "OnDemand24x", "OnDemand1xCuda"];
const WEIGHTS: readonly ZhiziWeight[] = ["10b512t", "10b384t", "11b768t", "28bnbt", "18bnbt", "20b", "40b", "60b", "fdx"];

export interface ZhiziAccountProps {
  backend: AccountBackend;
  labels: typeof en.remote;
  plan: ZhiziPlan;
  weight: ZhiziWeight;
  /** The main engine is 智子 with this plan and weight. */
  active: boolean;
  onConnect(plan: ZhiziPlan, weight: ZhiziWeight): Promise<void>;
  onWebsite(): void;
  onLoggedOut(status: ZhiziStatus): void;
}

type Details = { kind: "usage"; page: ZhiziUsagePage } | { kind: "credit"; page: ZhiziCreditPage };

/** The logged-in 智子 page: the account at a glance, its records, and the plan to connect with. */
export function ZhiziAccount({ backend, labels, plan: savedPlan, weight: savedWeight, active, onConnect, onWebsite, onLoggedOut }: ZhiziAccountProps) {
  const text = labels.zhizi;
  const [overview, setOverview] = useState<ZhiziOverview | null>(null);
  const [details, setDetails] = useState<Details | null>(null);
  const [plan, setPlan] = useState(savedPlan);
  const [weight, setWeight] = useState(savedWeight);
  const [busy, setBusy] = useState<"overview" | "details" | "connect" | null>("overview");
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(
    (kind: "overview" | "details" | "connect", work: () => Promise<void>) => {
      setBusy(kind);
      setError(null);
      work()
        .catch((failure: unknown) => setError(remoteError(failure, labels)))
        .finally(() => setBusy(null));
    },
    [labels],
  );
  const refresh = useCallback(
    (force: boolean) => run("overview", async () => setOverview(await backend.zhiziOverview!(force))),
    [backend, run],
  );
  // The cached overview when the page opens; 刷新 asks 智子 again.
  useEffect(() => {
    let active = true;
    backend
      .zhiziOverview?.(false)
      .then((loaded) => active && setOverview(loaded))
      .catch((failure: unknown) => active && setError(remoteError(failure, labels)))
      .finally(() => active && setBusy(null));
    return () => {
      active = false;
    };
  }, [backend, labels]);

  const page = (kind: Details["kind"], at: number) =>
    run("details", async () => setDetails(kind === "usage" ? { kind, page: await backend.zhiziUsages!(at) } : { kind, page: await backend.zhiziCredits!(at) }));

  const account = overview?.account;
  const balance = overview?.balance;
  const recent = overview?.recent_usage?.items?.[0];
  const unchanged = active && plan === savedPlan && weight === savedWeight;

  if (details) {
    const current = details.page;
    const pages = Math.max(1, Math.ceil((current.total ?? 0) / (current.page_size || 20)));
    const at = current.page ?? 0;
    return (
      <div className="zhizi-details" data-testid="zhizi-details">
        <div className="remote-row">
          <div className="segmented small" role="radiogroup">
            {(["usage", "credit"] as const).map((kind) => (
              <button key={kind} type="button" role="radio" aria-checked={details.kind === kind} data-testid={`zhizi-details-${kind}`} onClick={() => page(kind, 0)}>
                {kind === "usage" ? text.usageDetails : text.creditDetails}
              </button>
            ))}
          </div>
          <button type="button" className="link-button" data-testid="zhizi-details-back" onClick={() => setDetails(null)}>
            {text.backToAccount}
          </button>
        </div>
        <div className="zhizi-table" role="table">
          {details.kind === "usage" ? (
            <>
              <div className="zhizi-row zhizi-head" role="row">
                {[text.time, text.duration, text.cost, text.gpu, text.state].map((name) => (
                  <span key={name} role="columnheader">{name}</span>
                ))}
              </div>
              {(details.page.items ?? []).map((item, index) => (
                <div key={index} className="zhizi-row" role="row" data-testid="zhizi-record">
                  <span role="cell">{shortTime(item.started_at)}</span>
                  <span role="cell">{duration(item.duration_seconds, labels)}</span>
                  <span role="cell">{yuan(item.total_cost_yuan)}</span>
                  <span role="cell">{item.vip ? text.vip : (item.gpu_type ?? "")}</span>
                  <span role="cell">{item.finished ? text.finished : item.ready ? text.running : text.preparing}</span>
                </div>
              ))}
            </>
          ) : (
            <>
              <div className="zhizi-row zhizi-head" role="row">
                {[text.time, text.creditType, text.amount, text.source].map((name) => (
                  <span key={name} role="columnheader">{name}</span>
                ))}
              </div>
              {(details.page.items ?? []).map((item, index) => (
                <div key={index} className="zhizi-row" role="row" data-testid="zhizi-record">
                  <span role="cell">{shortTime(item.created_at)}</span>
                  <span role="cell">{item.credit_type ?? ""}</span>
                  <span role="cell">{yuan(item.amount_yuan)}</span>
                  <span role="cell">{item.product_name || item.source || ""}</span>
                </div>
              ))}
            </>
          )}
          {(details.page.items ?? []).length === 0 ? <p className="panel-empty">{text.noRecords}</p> : null}
        </div>
        <div className="remote-row zhizi-pages">
          <button type="button" className="secondary-button small" disabled={busy !== null || at <= 0} onClick={() => page(details.kind, at - 1)}>
            {text.previous}
          </button>
          <span>{text.page.replace("{page}", String(at + 1)).replace("{pages}", String(pages)).replace("{total}", String(current.total ?? 0))}</span>
          <button type="button" className="secondary-button small" disabled={busy !== null || at + 1 >= pages} onClick={() => page(details.kind, at + 1)}>
            {text.next}
          </button>
        </div>
        {error ? <p className="dialog-error" role="alert">{error}</p> : null}
      </div>
    );
  }

  return (
    <div className="zhizi-account" data-testid="zhizi-account">
      <section className="zhizi-card" aria-busy={busy === "overview"}>
        <div className="remote-row">
          <strong data-testid="zhizi-who">{text.accountValue.replace("{account}", account?.phone || account?.email || "--")}</strong>
          <span className={account?.membership ? "zhizi-vip on" : "zhizi-vip"}>
            {account?.membership ? text.vipActive : text.vipInactive}
            {account?.membership && account.membership_expires_at ? ` · ${text.expires.replace("{date}", shortTime(account.membership_expires_at).slice(0, 10))}` : ""}
          </span>
        </div>
        <p data-testid="zhizi-balance">
          {text.balanceSummary.replace("{balance}", yuan(balance?.remaining_balance_yuan)).replace("{yesterday}", yuan(balance?.yesterday_consumption_yuan))}
        </p>
        <p>{text.connections.replace("{n}", String(balance?.current_connections ?? 0))}</p>
        <p>{recent ? text.recentUsage.replace("{duration}", duration(recent.duration_seconds, labels)).replace("{cost}", yuan(recent.total_cost_yuan)) : text.noRecentUsage}</p>
        <div className="remote-row remote-buttons">
          <button type="button" className="secondary-button small" data-testid="zhizi-refresh" disabled={busy !== null} onClick={() => refresh(true)}>
            {busy === "overview" ? text.refreshing : text.refresh}
          </button>
          <button type="button" className="secondary-button small" data-testid="zhizi-usage" disabled={busy !== null} onClick={() => page("usage", 0)}>
            {text.usageDetails}
          </button>
          <button type="button" className="secondary-button small" data-testid="zhizi-credit" disabled={busy !== null} onClick={() => page("credit", 0)}>
            {text.creditDetails}
          </button>
          <button type="button" className="secondary-button small" data-testid="zhizi-recharge" onClick={onWebsite}>
            {text.recharge}
          </button>
          <button type="button" className="secondary-button small" data-testid="zhizi-vip" onClick={onWebsite}>
            {text.buyVip}
          </button>
        </div>
      </section>
      <div className="remote-grid">
        <label htmlFor="zhizi-plan">{text.plan}</label>
        <select id="zhizi-plan" data-testid="zhizi-plan" value={plan} onChange={(event) => setPlan(event.target.value as ZhiziPlan)}>
          {PLANS.map((value) => (
            <option key={value} value={value}>
              {text.plans[value]}
            </option>
          ))}
        </select>
        <label htmlFor="zhizi-weight">{text.weight}</label>
        <select id="zhizi-weight" data-testid="zhizi-weight" value={weight} onChange={(event) => setWeight(event.target.value as ZhiziWeight)}>
          {WEIGHTS.map((value) => (
            <option key={value} value={value}>
              {text.weights[value]}
            </option>
          ))}
        </select>
      </div>
      <p className="remote-hint">{text.billingNote}</p>
      {error ? (
        <p className="dialog-error" role="alert" data-testid="zhizi-error">
          {error}
        </p>
      ) : null}
      <div className="remote-actions">
        <button type="button" className="link-button" data-testid="zhizi-logout" disabled={busy !== null} onClick={() => run("overview", async () => onLoggedOut(await backend.zhiziLogout!()))}>
          {text.logout}
        </button>
        <button type="button" className="primary-button" data-testid="zhizi-connect" disabled={busy !== null || unchanged} onClick={() => run("connect", () => onConnect(plan, weight))}>
          {busy === "connect" ? text.connecting : unchanged ? text.enabled : active ? text.change : text.enable}
        </button>
      </div>
    </div>
  );
}
