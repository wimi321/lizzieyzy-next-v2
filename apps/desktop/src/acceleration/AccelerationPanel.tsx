import { useState } from "react";
import type { AssetDownloadStatus } from "@/api/backend";
import { type AccelerationLabels, type Failure, mb, onGpu, size } from "./format";
import type { Acceleration, Outcome, Run } from "./useAcceleration";

const fill = (text: string, values: Record<string, string | number>) => text.replace(/\{(\w+)\}/g, (whole, key: string) => (key in values ? String(values[key]) : whole));

/** KataGo's last lines, folded; they can be copied for a bug report. */
function Output({ tail, labels }: { tail: string[]; labels: AccelerationLabels }) {
  const [copied, setCopied] = useState(false);
  if (tail.length === 0) return null;
  const text = tail.join("\n");
  return (
    <details className="accel-output" data-testid="accel-output">
      <summary>{fill(labels.output, { n: tail.length })}</summary>
      <pre>{text}</pre>
      <button
        type="button"
        className="secondary-button"
        data-testid="accel-copy"
        onClick={() =>
          void navigator.clipboard
            ?.writeText(text)
            .then(() => setCopied(true))
            .catch(() => undefined)
        }
      >
        {copied ? labels.copied : labels.copy}
      </button>
    </details>
  );
}

function RunStatus({ acceleration, run, download, labels }: { acceleration: Acceleration; run: Run; download: AssetDownloadStatus | null; labels: AccelerationLabels }) {
  const name = labels.backends[run.backend];
  if (run.phase === "Downloading") {
    const total = download?.total_bytes ?? 0;
    const done = download?.downloaded_bytes ?? 0;
    const paused = download?.phase === "Paused";
    const text =
      download?.phase === "Installing"
        ? fill(labels.installing, { backend: name })
        : fill(paused ? labels.paused : labels.downloading, { backend: name, done: mb(done), total: mb(total) });
    return (
      <div className="tools-progress accel-run" data-testid="accel-run" data-phase={run.phase}>
        <div className="tools-progress-bar">
          <div style={{ width: total > 0 ? `${Math.min(100, (done / total) * 100)}%` : "0%" }} />
        </div>
        <span>{text}</span>
        {download?.phase === "Downloading" || paused ? (
          <button type="button" className="secondary-button" data-testid="accel-pause" onClick={paused ? acceleration.resume : acceleration.pause}>
            {paused ? labels.resume : labels.pause}
          </button>
        ) : null}
        <button type="button" className="secondary-button" data-testid="accel-cancel" onClick={acceleration.cancel}>
          {labels.cancel}
        </button>
      </div>
    );
  }
  const text =
    run.phase === "Tuning" && run.backend !== "cuda"
      ? fill(run.backend === "opencl" ? labels.tuningOpenCL : labels.tuningTensorRT, { seconds: acceleration.elapsed })
      : fill(labels.starting, { backend: name, seconds: acceleration.elapsed });
  return (
    <div className="accel-run" data-testid="accel-run" data-phase={run.phase}>
      <span className="accel-spinner" aria-hidden="true" />
      <span>{text}</span>
      <button type="button" className="secondary-button" data-testid="accel-cancel" onClick={acceleration.cancel}>
        {labels.cancel}
      </button>
    </div>
  );
}

function failureText(failure: Failure, labels: AccelerationLabels, backend: string): string {
  switch (failure.kind) {
    case "Failed":
    case "Other":
      return failure.reason;
    case "TimedOut":
      return fill(labels.timedOut, { backend });
    case "Busy":
      return labels.busy;
    case "NoEngine":
      return labels.noEngine;
    case "Unsupported":
      return labels.reasons.UnsupportedPlatform ?? "";
    case "Cancelled":
      return "";
  }
}

function OutcomeView({ acceleration, outcome, labels }: { acceleration: Acceleration; outcome: Outcome; labels: AccelerationLabels }) {
  if (outcome.type === "succeeded") {
    return (
      <div className="accel-outcome" data-testid="accel-outcome" data-outcome="succeeded">
        <span>{fill(labels.succeeded, { backend: labels.backends[outcome.backend] })}</span>
        <button type="button" className="secondary-button" data-testid="accel-benchmark" onClick={acceleration.openBenchmark}>
          {labels.benchmark}
        </button>
      </div>
    );
  }
  if (outcome.type === "restored") {
    return (
      <div className="accel-outcome" data-testid="accel-outcome" data-outcome="restored">
        {labels.restored}
      </div>
    );
  }
  const backend = outcome.type === "failed" ? labels.backends[outcome.backend] : "";
  const cancelled = outcome.failure.kind === "Cancelled";
  const detail = failureText(outcome.failure, labels, backend);
  const tail = "tail" in outcome.failure ? outcome.failure.tail : [];
  return (
    <div className="dialog-error accel-outcome" role="alert" data-testid="accel-outcome" data-outcome={cancelled ? "cancelled" : outcome.type}>
      {outcome.type === "failed" ? <strong>{cancelled ? labels.cancelled : labels.failed}</strong> : null}
      {detail ? <p>{detail}</p> : null}
      <Output tail={tail} labels={labels} />
    </div>
  );
}

/** 引擎一键设置 → 显卡提速: the GPUs, the engine in use, the choices and one enable at a time. */
export function AccelerationPanel({ acceleration, download, labels }: { acceleration: Acceleration; download: AssetDownloadStatus | null; labels: AccelerationLabels }) {
  const { plan, options, run, outcome } = acceleration;
  const gpus = (plan?.gpus ?? []).filter((gpu) => gpu.name);
  const working = run !== null || acceleration.busy;
  return (
    <div className="tools-panel accel" data-testid="tools-acceleration">
      <p className="dialog-intro">{labels.intro}</p>
      {plan === null ? (
        <p className="tools-loading">{labels.loading}</p>
      ) : (
        <>
          <dl className="accel-summary">
            <dt>{labels.gpus}</dt>
            <dd data-testid="accel-gpus">
              {gpus.length === 0
                ? labels.noGpu
                : gpus.map((gpu) => (
                    <span key={gpu.name} className="accel-gpu">
                      {gpu.driver_version ? fill(labels.driver, { name: gpu.name ?? "", version: gpu.driver_version }) : gpu.name}
                    </span>
                  ))}
            </dd>
            <dt>{labels.current}</dt>
            <dd data-testid="accel-current">{labels.backends[plan.current ?? "external"]}</dd>
          </dl>
          {options.length === 0 ? (
            <p className="tools-reason" data-reason={plan.reason}>
              {labels.reasons[plan.reason ?? ""] ?? plan.reason}
            </p>
          ) : (
            <div className="accel-options" role="radiogroup" aria-label={labels.gpus}>
              {options.map((option) => {
                const inUse = option.backend === plan.current;
                return (
                  <label key={option.backend} className="accel-option" data-testid={`accel-option-${option.backend}`} data-disabled={!option.supported || undefined}>
                    <input
                      type="radio"
                      name="accel-backend"
                      checked={acceleration.selected === option.backend}
                      disabled={!option.supported || inUse || working}
                      onChange={() => acceleration.choose(option.backend)}
                    />
                    <span className="accel-option-text">
                      <span className="accel-option-title">
                        <strong>{labels.backends[option.backend]}</strong>
                        {option.recommended && option.supported ? <span className="accel-tag">{labels.recommended}</span> : null}
                        {inUse ? <span className="engine-main-tag">{labels.inUse}</span> : null}
                        <span className="accel-size">{option.installed ? labels.installed : fill(labels.download, { size: size(option.download_bytes) })}</span>
                      </span>
                      <span className="accel-option-detail">{option.supported ? labels.describe[option.backend] : labels.unsupported[option.backend]}</span>
                      {option.note === "DriverMayBeTooOld" && option.supported ? <span className="accel-note">{labels.oldDriver}</span> : null}
                    </span>
                  </label>
                );
              })}
            </div>
          )}
          {run ? <RunStatus acceleration={acceleration} run={run} download={download} labels={labels} /> : outcome ? <OutcomeView acceleration={acceleration} outcome={outcome} labels={labels} /> : null}
          {options.length > 0 || onGpu(plan) ? (
            <footer className="dialog-footer">
              {onGpu(plan) ? (
                <button type="button" className="secondary-button" data-testid="accel-disable" disabled={working} onClick={acceleration.disable}>
                  {labels.disable}
                </button>
              ) : null}
              {options.length > 0 ? (
                <button type="button" className="primary-button" data-testid="accel-enable" disabled={working || acceleration.selected === null} onClick={acceleration.enable}>
                  {labels.enable}
                </button>
              ) : null}
            </footer>
          ) : null}
        </>
      )}
    </div>
  );
}

/** The one-time hint in the main window while the bundled CPU engine runs on a GPU machine. */
export function AccelerationHint({ acceleration, labels }: { acceleration: Acceleration; labels: AccelerationLabels }) {
  if (!acceleration.hint) return null;
  return (
    <div className="accel-hint" role="status" data-testid="accel-hint">
      <span className="accel-hint-text">{fill(labels.hint, { gpu: acceleration.hint })}</span>
      <button type="button" className="accel-hint-close" aria-label={labels.close} data-testid="accel-hint-close" onClick={acceleration.dismissHint}>
        ×
      </button>
      <span className="accel-hint-actions">
        <button type="button" className="primary-button" data-testid="accel-hint-open" onClick={acceleration.openFromHint}>
          {labels.hintOpen}
        </button>
        <button type="button" className="secondary-button" data-testid="accel-hint-dismiss" onClick={acceleration.dismissHint}>
          {labels.hintDismiss}
        </button>
      </span>
    </div>
  );
}
