import type { DiagnosticCheck, OfficialWeight } from "@/api/backend";
import type { EngineTools, ToolsTab } from "./useEngineTools";

export interface ToolsLabels {
  title: string;
  tabs: Record<ToolsTab, string>;
  overviewIntro: string;
  kinds: Record<string, string>;
  statuses: Record<string, string>;
  repairs: Record<string, string>;
  recheck: string;
  noEngine: string;
  weightsIntro: string;
  size: string;
  published: string;
  downloaded: string;
  download: string;
  use: string;
  inUse: string;
  cancel: string;
  progress: string;
  installing: string;
  failed: string;
  accelerationIntro: string;
  reasons: Record<string, string>;
  install: string;
  loading: string;
}

const mb = (bytes: number | undefined) => ((bytes ?? 0) / 1_048_576).toFixed(1);

/** The 引擎一键设置 tab strip; 性能优化 is the benchmark itself. */
export function ToolsTabs({ tab, labels, onTab }: { tab: ToolsTab; labels: ToolsLabels; onTab(tab: ToolsTab): void }) {
  const tabs: ToolsTab[] = ["overview", "weights", "acceleration", "benchmark"];
  return (
    <div className="tools-tabs" role="tablist">
      {tabs.map((name) => (
        <button key={name} type="button" role="tab" aria-selected={tab === name} data-testid={`tools-tab-${name}`} onClick={() => onTab(name)}>
          {labels.tabs[name]}
        </button>
      ))}
    </div>
  );
}

function Progress({ tools, labels }: { tools: EngineTools; labels: ToolsLabels }) {
  const status = tools.download;
  if (!status || !tools.downloading) return null;
  const total = status.total_bytes ?? 0;
  const done = status.downloaded_bytes ?? 0;
  return (
    <div className="tools-progress" data-testid="tools-progress">
      <div className="tools-progress-bar">
        <div style={{ width: total > 0 ? `${Math.min(100, (done / total) * 100)}%` : "0%" }} />
      </div>
      <span>{status.phase === "Installing" ? labels.installing : labels.progress.replace("{done}", mb(done)).replace("{total}", mb(total))}</span>
      <button type="button" className="secondary-button" data-testid="tools-cancel" disabled={tools.busy} onClick={tools.cancelDownload}>
        {labels.cancel}
      </button>
    </div>
  );
}

function Errors({ tools, labels }: { tools: EngineTools; labels: ToolsLabels }) {
  const message = tools.error ?? (tools.download?.phase === "Failed" ? (tools.download.error ?? labels.failed) : null);
  return message ? (
    <div className="dialog-error" role="alert" data-testid="tools-error">
      <pre>{message}</pre>
    </div>
  ) : null;
}

/** 概览: each check of the main engine, with the repairs Rust offers for it. */
export function OverviewPanel({ tools, labels }: { tools: EngineTools; labels: ToolsLabels }) {
  const checks = tools.diagnostics?.checks ?? [];
  return (
    <div className="tools-panel" data-testid="tools-overview">
      <p className="dialog-intro">{tools.diagnostics?.profile === null ? labels.noEngine : labels.overviewIntro}</p>
      {tools.diagnostics === null ? <p className="tools-loading">{labels.loading}</p> : null}
      <ul className="tools-checks">
        {checks.map((check: DiagnosticCheck) => (
          <li key={check.kind} className="tools-check" data-testid="tools-check" data-status={check.status}>
            <span className="tools-check-dot" aria-hidden="true" />
            <span className="tools-check-text">
              <strong>{labels.kinds[check.kind ?? ""] ?? check.kind}</strong>
              <span className="tools-check-status">{labels.statuses[check.status ?? ""] ?? check.status}</span>
              {check.detail || check.path ? <span className="tools-check-detail" title={check.path ?? undefined}>{check.detail ?? check.path}</span> : null}
            </span>
            <span className="tools-check-actions">
              {(check.repairs ?? []).map((repair) => (
                <button key={repair} type="button" className="secondary-button" data-testid={`tools-repair-${repair}`} disabled={tools.busy || tools.downloading} onClick={() => tools.repair(repair)}>
                  {labels.repairs[repair] ?? repair}
                </button>
              ))}
            </span>
          </li>
        ))}
      </ul>
      <Progress tools={tools} labels={labels} />
      <Errors tools={tools} labels={labels} />
      <footer className="dialog-footer">
        <button type="button" className="secondary-button" data-testid="tools-recheck" disabled={tools.busy} onClick={tools.diagnose}>
          {labels.recheck}
        </button>
      </footer>
    </div>
  );
}

/** 权重管理: the official weights; a downloaded one can become the main engine's. */
export function WeightsPanel({ tools, labels, currentModel }: { tools: EngineTools; labels: ToolsLabels; currentModel: string | null }) {
  const row = (weight: OfficialWeight) => {
    const inUse = weight.path !== null && weight.path !== undefined && weight.path === currentModel;
    const active = tools.downloading && tools.download?.asset_id === weight.id;
    return (
      <li key={weight.id} className="tools-weight" data-testid="tools-weight" data-downloaded={weight.downloaded || undefined}>
        <span className="tools-weight-text">
          <strong>{weight.name}</strong>
          <span>
            {[labels.size.replace("{n}", mb(weight.size_bytes)), weight.published_at ? labels.published.replace("{date}", weight.published_at) : null, weight.downloaded ? labels.downloaded : null]
              .filter(Boolean)
              .join(" · ")}
          </span>
        </span>
        {inUse ? (
          <span className="engine-main-tag">{labels.inUse}</span>
        ) : weight.downloaded ? (
          <button type="button" className="secondary-button" data-testid="tools-use" disabled={tools.busy} onClick={() => weight.id && tools.chooseWeight(weight.id)}>
            {labels.use}
          </button>
        ) : (
          <button type="button" className="primary-button" data-testid="tools-download" disabled={tools.busy || tools.downloading} onClick={() => weight.id && tools.startDownload(weight.id)}>
            {active ? labels.installing : labels.download}
          </button>
        )}
      </li>
    );
  };
  return (
    <div className="tools-panel" data-testid="tools-weights">
      <p className="dialog-intro">{labels.weightsIntro}</p>
      {tools.weights === null ? <p className="tools-loading">{labels.loading}</p> : null}
      <ul className="tools-weights">{(tools.weights ?? []).map(row)}</ul>
      <Progress tools={tools} labels={labels} />
      <Errors tools={tools} labels={labels} />
    </div>
  );
}

/** 英伟达显卡提速组件: whether this machine needs them, and installing them when it does. */
export function AccelerationPanel({ tools, labels }: { tools: EngineTools; labels: ToolsLabels }) {
  const reason = tools.plan?.reason;
  return (
    <div className="tools-panel" data-testid="tools-acceleration">
      <p className="dialog-intro">{labels.accelerationIntro}</p>
      {tools.plan === null ? <p className="tools-loading">{labels.loading}</p> : <p className="tools-reason" data-reason={reason}>{labels.reasons[reason ?? ""] ?? reason}</p>}
      <Progress tools={tools} labels={labels} />
      <Errors tools={tools} labels={labels} />
      {reason === "NvidiaDetected" ? (
        <footer className="dialog-footer">
          <button type="button" className="primary-button" data-testid="tools-install" disabled={tools.busy || tools.downloading} onClick={() => tools.repair("InstallRuntime")}>
            {labels.install}
          </button>
        </footer>
      ) : null}
    </div>
  );
}

/** The panel of a tab other than 性能优化, which is the benchmark itself. */
export function ToolsPanel({ tab, tools, labels, currentModel }: { tab: Exclude<ToolsTab, "benchmark">; tools: EngineTools; labels: ToolsLabels; currentModel: string | null }) {
  if (tab === "overview") return <OverviewPanel tools={tools} labels={labels} />;
  if (tab === "weights") return <WeightsPanel tools={tools} labels={labels} currentModel={currentModel} />;
  return <AccelerationPanel tools={tools} labels={labels} />;
}
