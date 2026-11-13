import { type FormEvent, useEffect, useState } from "react";
import type { AnalysisLimitSettings } from "@/api/backend";
import { ENGINE_DEFAULTS, type EngineOptions } from "../engines/model";

export interface SettingsLabels {
  title: string;
  analysis: string;
  limitTime: string;
  limitTimeHint: string;
  seconds: string;
  limitPlayout: string;
  limitPlayoutHint: string;
  visits: string;
  autoCurve: string;
  autoCurveHint: string;
  cache: string;
  cacheHint: string;
  engine: string;
  readKomi: string;
  readKomiHint: string;
  defaultKomi: string;
  checkAlive: string;
  checkAliveHint: string;
  interval: string;
  intervalHint: string;
  milliseconds: string;
  updates: string;
  autoCheckUpdates: string;
  autoCheckUpdatesHint: string;
  invalid: string;
  save: string;
  cancel: string;
  close: string;
}

export interface SettingsValues {
  limits: AnalysisLimitSettings;
  autoCurve: boolean;
  /** 启用缓存: analysed positions are kept on disk and shown again without recomputing. */
  cache: boolean;
  /** 读取棋谱贴目, 自动检测引擎是否存活, 本地分析刷新间隔 (P2-11). */
  engine?: EngineOptions;
  /** 启动时自动检查更新 (P2-14); absent where the dialog is shown without it. */
  autoCheckUpdates?: boolean;
}

export interface SettingsDialogProps {
  values: SettingsValues;
  labels: SettingsLabels;
  /** Saves what changed; rejects with the reason when it cannot. */
  onSave(values: SettingsValues): Promise<void>;
  onClose(): void;
}

// The settings file keeps the time limit as a u32; visit limits fit any safe integer.
const MAX_SECONDS = 0xffff_ffff;

/** Java's refresh interval runs from 10 ms to 10 s. */
const INTERVAL = { min: 10, max: 10_000 };

function komiOf(text: string): number | null {
  const value = Number(text.trim());
  return text.trim() !== "" && Number.isFinite(value) && Math.abs(value) <= 1000 && Number.isInteger(value * 2) ? value : null;
}

function positive(text: string, max = Number.MAX_SAFE_INTEGER): number | null {
  if (!/^\d+$/.test(text.trim())) return null;
  const value = Number(text.trim());
  return value > 0 && value <= max ? value : null;
}

/** 设置 → 综合设置, for now the analysis page of LizzieYzy Next's ConfigDialog2. */
export function SettingsDialog({ values, labels, onSave, onClose }: SettingsDialogProps) {
  const [limitTime, setLimitTime] = useState(values.limits.limit_time);
  const [seconds, setSeconds] = useState(String(values.limits.max_analyze_time_seconds));
  const [limitPlayout, setLimitPlayout] = useState(values.limits.limit_playout);
  const [playouts, setPlayouts] = useState(String(values.limits.limit_playouts));
  const [autoCurve, setAutoCurve] = useState(values.autoCurve);
  const [cache, setCache] = useState(values.cache);
  const engine = values.engine ?? ENGINE_DEFAULTS;
  const [readKomi, setReadKomi] = useState(engine.read_komi);
  const [komi, setKomi] = useState(String(engine.default_komi));
  const [checkAlive, setCheckAlive] = useState(engine.auto_check_alive);
  const [interval, setInterval] = useState(String(engine.report_interval_ms));
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(values.autoCheckUpdates);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const secondsValue = positive(seconds, MAX_SECONDS);
  const playoutsValue = positive(playouts);
  const komiValue = komiOf(komi);
  const intervalValue = positive(interval, INTERVAL.max);
  const intervalOk = intervalValue !== null && intervalValue >= INTERVAL.min;
  const valid = (!limitTime || secondsValue !== null) && (!limitPlayout || playoutsValue !== null) && komiValue !== null && intervalOk;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    setBusy(true);
    setError(null);
    onSave({
      limits: {
        limit_time: limitTime,
        max_analyze_time_seconds: secondsValue ?? values.limits.max_analyze_time_seconds,
        limit_playout: limitPlayout,
        limit_playouts: playoutsValue ?? values.limits.limit_playouts,
      },
      autoCurve,
      cache,
      engine: {
        ...engine,
        read_komi: readKomi,
        default_komi: komiValue ?? engine.default_komi,
        auto_check_alive: checkAlive,
        report_interval_ms: intervalValue ?? engine.report_interval_ms,
      },
      autoCheckUpdates,
    }).catch((reason: unknown) => {
      setBusy(false);
      setError(typeof reason === "object" && reason !== null && "data" in reason ? String((reason as { data: unknown }).data) : String(reason));
    });
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog settings-dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title" data-testid="settings-dialog">
        <header className="dialog-header">
          <h2 id="settings-title">{labels.title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} data-testid="settings-close" onClick={onClose}>
            ×
          </button>
        </header>
        <form className="settings-form" noValidate onSubmit={submit}>
          {/* Only the settings scroll; 取消 and 保存 stay in view. */}
          <div className="settings-body" data-testid="settings-body">
            <h3 className="settings-section">{labels.analysis}</h3>
            <div className="settings-row">
              <label className="settings-check">
                <input type="checkbox" data-testid="settings-limit-time" checked={limitTime} onChange={(event) => setLimitTime(event.target.checked)} />
                <span>{labels.limitTime}</span>
              </label>
              <span className="settings-field">
                <input
                  className="text-input"
                  inputMode="numeric"
                  data-testid="settings-seconds"
                  aria-label={labels.limitTime}
                  aria-invalid={limitTime && secondsValue === null}
                  disabled={!limitTime}
                  value={seconds}
                  onChange={(event) => setSeconds(event.target.value)}
                />
                <span>{labels.seconds}</span>
              </span>
              <p className="settings-hint">{labels.limitTimeHint}</p>
            </div>
            <div className="settings-row">
              <label className="settings-check">
                <input type="checkbox" data-testid="settings-limit-playout" checked={limitPlayout} onChange={(event) => setLimitPlayout(event.target.checked)} />
                <span>{labels.limitPlayout}</span>
              </label>
              <span className="settings-field">
                <input
                  className="text-input"
                  inputMode="numeric"
                  data-testid="settings-playouts"
                  aria-label={labels.limitPlayout}
                  aria-invalid={limitPlayout && playoutsValue === null}
                  disabled={!limitPlayout}
                  value={playouts}
                  onChange={(event) => setPlayouts(event.target.value)}
                />
                <span>{labels.visits}</span>
              </span>
              <p className="settings-hint">{labels.limitPlayoutHint}</p>
            </div>
            <div className="settings-row">
              <label className="settings-check">
                <input type="checkbox" data-testid="settings-auto-curve" checked={autoCurve} onChange={(event) => setAutoCurve(event.target.checked)} />
                <span>{labels.autoCurve}</span>
              </label>
              <p className="settings-hint">{labels.autoCurveHint}</p>
            </div>
            <div className="settings-row">
              <label className="settings-check">
                <input type="checkbox" data-testid="settings-cache" checked={cache} onChange={(event) => setCache(event.target.checked)} />
                <span>{labels.cache}</span>
              </label>
              <p className="settings-hint">{labels.cacheHint}</p>
            </div>
            <h3 className="settings-section">{labels.engine}</h3>
            <div className="settings-row">
              <label className="settings-check">
                <input type="checkbox" data-testid="settings-read-komi" checked={readKomi} onChange={(event) => setReadKomi(event.target.checked)} />
                <span>{labels.readKomi}</span>
              </label>
              <span className="settings-field">
                <span>{labels.defaultKomi}</span>
                <input
                  className="text-input"
                  inputMode="decimal"
                  data-testid="settings-default-komi"
                  aria-label={labels.defaultKomi}
                  aria-invalid={komiValue === null}
                  value={komi}
                  onChange={(event) => setKomi(event.target.value)}
                />
              </span>
              <p className="settings-hint">{labels.readKomiHint}</p>
            </div>
            <div className="settings-row">
              <label className="settings-check">
                <input type="checkbox" data-testid="settings-check-alive" checked={checkAlive} onChange={(event) => setCheckAlive(event.target.checked)} />
                <span>{labels.checkAlive}</span>
              </label>
              <p className="settings-hint">{labels.checkAliveHint}</p>
            </div>
            <div className="settings-row">
              <span className="settings-label">{labels.interval}</span>
              <span className="settings-field">
                <input
                  className="text-input"
                  inputMode="numeric"
                  data-testid="settings-interval"
                  aria-label={labels.interval}
                  aria-invalid={!intervalOk}
                  value={interval}
                  onChange={(event) => setInterval(event.target.value)}
                />
                <span>{labels.milliseconds}</span>
              </span>
              <p className="settings-hint">{labels.intervalHint}</p>
            </div>
            {autoCheckUpdates !== undefined ? (
              <>
                <h3 className="settings-section">{labels.updates}</h3>
                <div className="settings-row">
                  <label className="settings-check">
                    <input type="checkbox" data-testid="settings-auto-check-updates" checked={autoCheckUpdates} onChange={(event) => setAutoCheckUpdates(event.target.checked)} />
                    <span>{labels.autoCheckUpdates}</span>
                  </label>
                  <p className="settings-hint">{labels.autoCheckUpdatesHint}</p>
                </div>
              </>
            ) : null}
            {!valid ? (
              <p className="settings-invalid" role="alert" data-testid="settings-invalid">
                {labels.invalid}
              </p>
            ) : null}
            {error ? (
              <div className="dialog-error" role="alert" data-testid="settings-error">
                <pre>{error}</pre>
              </div>
            ) : null}
          </div>
          <footer className="dialog-footer settings-footer">
            <button type="button" className="secondary-button" data-testid="settings-cancel" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="settings-save" disabled={busy || !valid}>
              {labels.save}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
