import type { WebBoardStatus } from "@/api/backend";

export interface WebBoardLabels {
  chip: string;
  chipTrial: string;
  endTrial: string;
  title: string;
  intro: string;
  addresses: string;
  noAddress: string;
  copy: string;
  copied: string;
  copiedUrl: string;
  copyFailed: string;
  viewers: string;
  trialNone: string;
  trialActive: string;
  allowTrial: string;
  stop: string;
  close: string;
  firewall: string;
  startFailed: string;
  failed: string;
}

const fill = (text: string, values: Record<string, string | number>) => text.replace(/\{(\w+)\}/g, (whole, key: string) => (key in values ? String(values[key]) : whole));

export interface WebBoardChipProps {
  status: WebBoardStatus;
  labels: WebBoardLabels;
  open: boolean;
  /** Minutes since the viewer's trial began, when one runs. */
  trialMinutes: number | null;
  allowTrial: boolean;
  windows: boolean;
  onToggle(): void;
  onClose(): void;
  onCopy(url: string): void;
  onAllowTrial(): void;
  onForceEnd(): void;
  onStop(): void;
}

/**
 * The strip's 网页旁观 chip: the number of viewers, or a viewer's trial with 结束试下; it opens the
 * addresses, the viewers and the trial. The URL in Java's window title becomes this.
 */
export function WebBoardChip({ status, labels, open, trialMinutes, allowTrial, windows, onToggle, onClose, onCopy, onAllowTrial, onForceEnd, onStop }: WebBoardChipProps) {
  const trial = trialMinutes !== null;
  const addresses = status.addresses?.length ? status.addresses : status.url ? [status.url] : [];
  return (
    <>
      <span className={`chip web-board-chip${trial ? " trial" : ""}`} data-testid="web-board-chip" data-trial={trial || undefined}>
        <button type="button" className="chip-label web-board-chip-open" aria-expanded={open} data-testid="web-board-chip-open" onClick={onToggle}>
          {trial ? labels.chipTrial : fill(labels.chip, { n: status.viewers ?? 0 })}
        </button>
        {trial ? (
          <button type="button" className="tsumego-exit" data-testid="web-board-chip-end" onClick={onForceEnd}>
            {labels.endTrial}
          </button>
        ) : null}
      </span>
      {open ? (
        <section className="web-board-panel" role="dialog" aria-label={labels.title} data-testid="web-board-panel">
          <header>
            <h2>{labels.title}</h2>
            <button type="button" className="dialog-close" aria-label={labels.close} data-testid="web-board-panel-close" onClick={onClose}>
              ×
            </button>
          </header>
          <p className="web-board-intro">{labels.intro}</p>
          <h3>{labels.addresses}</h3>
          {addresses.length > 0 ? (
            <ul className="web-board-addresses">
              {addresses.map((url, index) => (
                <li key={url} data-primary={index === 0 || undefined}>
                  <input readOnly value={url} data-testid="web-board-address" onFocus={(event) => event.currentTarget.select()} />
                  <button type="button" className="secondary-button" data-testid="web-board-copy" onClick={() => onCopy(url)}>
                    {labels.copy}
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="web-board-muted">{labels.noAddress}</p>
          )}
          <p className="web-board-line" data-testid="web-board-viewers">
            {fill(labels.viewers, { n: status.viewers ?? 0 })}
          </p>
          <p className="web-board-line" data-testid="web-board-trial">
            <span>{trial ? fill(labels.trialActive, { minutes: trialMinutes }) : labels.trialNone}</span>
            {trial ? (
              <button type="button" className="secondary-button" data-testid="web-board-force-end" onClick={onForceEnd}>
                {labels.endTrial}
              </button>
            ) : null}
          </p>
          <label className="jt-check web-board-allow">
            <input type="checkbox" checked={allowTrial} data-testid="web-board-allow" onChange={onAllowTrial} />
            <span>{labels.allowTrial}</span>
          </label>
          {windows ? <p className="web-board-muted">{labels.firewall}</p> : null}
          <footer>
            <button type="button" className="secondary-button" data-testid="web-board-stop" onClick={onStop}>
              {labels.stop}
            </button>
          </footer>
        </section>
      ) : null}
    </>
  );
}
