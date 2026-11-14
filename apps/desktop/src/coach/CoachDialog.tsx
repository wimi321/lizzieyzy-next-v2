import { type FormEvent, useState } from "react";
import type { CoachColor, CoachMode, CoachPace, CoachSetup, HumanSlModelStatus } from "../api/coach";
import type { AssetDownloadStatus } from "../api/generated/AssetDownloadStatus";
import { PlayWindow, Segmented } from "../play/PlayDialog";
import { COACH_DEFAULTS, type CoachLabels, komiFor, RANK_STEPS, rankAt, rankIndex, rankName } from "./rank";

interface CoachDialogProps {
  saved: CoachSetup | null;
  /** null while the status is being read. */
  status: HumanSlModelStatus | null;
  download: AssetDownloadStatus | null;
  /** The main engine is restarting with the model, or the game is being set up. */
  preparing: boolean;
  error: string | null;
  canContinue: boolean;
  labels: CoachLabels;
  onStart(setup: CoachSetup): void;
  onDownload(setup: CoachSetup): void;
  onPause(): void;
  onCancelDownload(): void;
  onImport(): void;
  onClose(): void;
}

const MODES: readonly CoachMode[] = ["LiveFeedback", "ReviewAfter", "LiveAnalysis"];
const COLORS: readonly CoachColor[] = ["Black", "White", "Random"];
const PACES: readonly CoachPace[] = ["Natural", "Fast", "Slow"];
const MB = 1024 * 1024;

/** 棋局 → 新对局 → AI 陪练, and the menu strip's AI 陪练 button: Java's coach setup. */
export function CoachDialog({ saved, status, download, preparing, error, canContinue, labels, onStart, onDownload, onPause, onCancelDownload, onImport, onClose }: CoachDialogProps) {
  const [setup, setSetup] = useState<Required<CoachSetup>>({ ...COACH_DEFAULTS, ...saved, from_current_position: false });
  const [more, setMore] = useState(false);
  const set = (patch: Partial<CoachSetup>) => setSetup((current) => ({ ...current, ...patch }));
  const index = rankIndex(setup.rank);
  const downloading = download?.asset_id === "human-sl" && (download.phase === "Downloading" || download.phase === "Installing");
  const paused = (download?.asset_id === "human-sl" && download.phase === "Paused") || (status?.type === "Downloading" && status.data.paused === true);
  const ready = status?.type === "Ready";
  const busy = preparing || downloading || status === null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    const chosen = { ...setup, color: setup.handicap > 0 ? "Black" : setup.color };
    if (ready) onStart(chosen);
    else onDownload(chosen);
  };

  const done = download?.downloaded_bytes ?? (status?.type === "Downloading" ? (status.data.downloaded_bytes ?? 0) : 0);
  const total = download?.total_bytes || (status?.type === "Downloading" ? status.data.total_bytes : 0) || 99_066_230;
  const share = total > 0 ? Math.min(1, done / total) : 0;
  const modelLine = (() => {
    if (status === null) return labels.modelChecking;
    if (status.type === "Ready") return labels.modelReady.replace("{size}", `${Math.round(99_066_230 / MB)} MB`);
    if (status.type === "Corrupt") return labels.modelCorrupt;
    return labels.modelMissing;
  })();

  return (
    <PlayWindow title={labels.title} testId="coach-dialog" className="coach-dialog" onClose={onClose} close={labels.close}>
      <form className="play-form" noValidate onSubmit={submit}>
        <div className="play-panel coach-rank">
          <div className="play-row">
            <span className="play-label">{labels.rank}</span>
            <span className="coach-rank-control">
              <button type="button" className="secondary-button coach-step" aria-label={labels.weaker} disabled={index <= 0} onClick={() => set({ rank: rankAt(index - 1) })}>
                −
              </button>
              <input type="range" min={0} max={RANK_STEPS - 1} step={1} value={index} aria-label={labels.rank} data-testid="coach-rank" onChange={(event) => set({ rank: rankAt(Number(event.target.value)) })} />
              <button type="button" className="secondary-button coach-step" aria-label={labels.stronger} disabled={index >= RANK_STEPS - 1} onClick={() => set({ rank: rankAt(index + 1) })}>
                +
              </button>
              <strong className="coach-rank-name" data-testid="coach-rank-name">
                {rankName(setup.rank, labels)}
              </strong>
            </span>
          </div>
          <div className="play-row play-top">
            <span className="play-label">{labels.mode}</span>
            <div className="coach-modes" role="radiogroup" aria-label={labels.mode}>
              {MODES.map((mode) => (
                <button key={mode} type="button" role="radio" aria-checked={setup.mode === mode} className="coach-mode" data-testid={`coach-mode-${mode}`} onClick={() => set({ mode })}>
                  <strong>{labels.modes[mode]}</strong>
                  <span>{labels.modeHints[mode]}</span>
                </button>
              ))}
            </div>
          </div>
          <div className="play-row">
            <span className="play-label">{labels.color}</span>
            <Segmented
              value={setup.handicap > 0 ? "Black" : setup.color}
              label={labels.color}
              testId="coach-color"
              onChange={(color) => set({ color })}
              options={COLORS.map((color) => ({ value: color, label: labels.colors[color], disabled: setup.handicap > 0 && color !== "Black" }))}
            />
          </div>
          <div className="play-row">
            <span className="play-label">{labels.pace}</span>
            <Segmented value={setup.pace} label={labels.pace} testId="coach-pace" onChange={(pace) => set({ pace })} options={PACES.map((pace) => ({ value: pace, label: labels.paces[pace] }))} />
          </div>
        </div>

        <button type="button" className="play-more-toggle" aria-expanded={more} onClick={() => setMore((open) => !open)}>
          {more ? "▾" : "▸"} {labels.more}
        </button>
        {more ? (
          <div className="play-panel">
            <div className="play-row">
              <span className="play-label">{labels.handicap}</span>
              <span className="play-inline">
                <select className="select" value={setup.handicap} disabled={setup.from_current_position} data-testid="coach-handicap" onChange={(event) => set({ handicap: Number(event.target.value) })}>
                  {[0, 2, 3, 4, 5, 6, 7, 8, 9].map((count) => (
                    <option key={count} value={count}>
                      {count === 0 ? labels.noHandicap : count}
                    </option>
                  ))}
                </select>
                <span className="play-unit">{labels.komi.replace("{komi}", String(komiFor(setup.handicap)))}</span>
              </span>
            </div>
            <label className="settings-check">
              <input type="checkbox" checked={setup.from_current_position} disabled={!canContinue} data-testid="coach-from-current" onChange={(event) => set({ from_current_position: event.target.checked })} />
              <span>{labels.fromCurrent}</span>
            </label>
          </div>
        ) : null}

        <div className="coach-model" data-testid="coach-model" data-state={status?.type ?? "Checking"}>
          <span className={ready ? "coach-model-dot ready" : "coach-model-dot"} aria-hidden="true" />
          <div className="coach-model-text">
            <span>{preparing ? labels.preparing : modelLine}</span>
            {downloading || paused ? (
              <span className="coach-progress" data-testid="coach-progress">
                <span className="coach-bar">
                  <span style={{ width: `${share * 100}%` }} />
                </span>
                <small>
                  {labels.downloading
                    .replace("{percent}", String(Math.floor(share * 100)))
                    .replace("{done}", (done / MB).toFixed(1))
                    .replace("{total}", (total / MB).toFixed(1))}
                </small>
              </span>
            ) : null}
          </div>
          {downloading ? (
            <span className="coach-model-actions">
              <button type="button" className="secondary-button" data-testid="coach-pause" onClick={onPause}>
                {labels.pause}
              </button>
              <button type="button" className="danger-link" onClick={onCancelDownload}>
                {labels.cancel}
              </button>
            </span>
          ) : !ready && status !== null ? (
            <button type="button" className="secondary-button" data-testid="coach-import" onClick={onImport}>
              {labels.import}
            </button>
          ) : null}
        </div>

        <footer className="dialog-footer settings-footer">
          {error ? (
            <p className="play-error coach-error" role="alert" data-testid="coach-error" title={error}>
              {error}
            </p>
          ) : null}
          <button type="button" className="secondary-button" onClick={onClose}>
            {labels.cancel}
          </button>
          <button type="submit" className="primary-button" data-testid="coach-start" disabled={busy}>
            {preparing ? labels.buttonPreparing : ready ? labels.start : paused ? labels.resumeStart : labels.downloadStart}
          </button>
        </footer>
      </form>
    </PlayWindow>
  );
}
