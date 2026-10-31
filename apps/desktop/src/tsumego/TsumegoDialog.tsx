import { type FormEvent, useEffect, useState } from "react";
import type { TsumegoAttacker, TsumegoKoThreat, TsumegoOptions, TsumegoToPlay } from "@/api/backend";

export interface TsumegoLabels {
  title: string;
  intro: string;
  toPlay: string;
  toPlayOptions: Record<TsumegoToPlay, string>;
  attacker: string;
  attackerOptions: Record<TsumegoAttacker, string>;
  koThreat: string;
  koThreatOptions: Record<TsumegoKoThreat, string>;
  wallDistance: string;
  wallDistanceHint: string;
  confirm: string;
  cancel: string;
  close: string;
  failed: string;
}

export interface TsumegoDialogProps {
  /** The options last confirmed, as the settings keep them. */
  options: TsumegoOptions;
  labels: TsumegoLabels;
  /** Generates the problem; rejects with the reason when it cannot. */
  onConfirm(options: TsumegoOptions): Promise<void>;
  onClose(): void;
}

const TO_PLAY: readonly TsumegoToPlay[] = ["Inherit", "Black", "White"];
const ATTACKER: readonly TsumegoAttacker[] = ["Auto", "Black", "White"];
const KO_THREAT: readonly TsumegoKoThreat[] = ["Both", "Attacker", "Defender", "None"];

/**
 * LizzieYzy Next's 死活题 window (`TsumeGoFrame`): who plays first, who attacks, which side gets
 * ko threats and how far the wall stands. Confirming frames the current position.
 */
export function TsumegoDialog({ options, labels, onConfirm, onClose }: TsumegoDialogProps) {
  const [toPlay, setToPlay] = useState(options.to_play);
  const [attacker, setAttacker] = useState(options.attacker);
  const [koThreat, setKoThreat] = useState(options.ko_threat);
  // Java keeps the old distance when the field is not a positive number.
  const [distance, setDistance] = useState(String(options.wall_distance));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const parsed = Number.parseInt(distance, 10);
    const wall_distance = Number.isInteger(parsed) && parsed > 0 ? parsed : options.wall_distance;
    setBusy(true);
    setError(null);
    onConfirm({ to_play: toPlay, attacker, ko_threat: koThreat, wall_distance }).catch((reason: unknown) => {
      setBusy(false);
      setError(describe(reason));
    });
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog tsumego-dialog" role="dialog" aria-modal="true" aria-labelledby="tsumego-title" data-testid="tsumego-dialog">
        <header className="dialog-header">
          <div>
            <h2 id="tsumego-title">{labels.title}</h2>
            <p className="tsumego-intro">{labels.intro}</p>
          </div>
          <button type="button" className="dialog-close" aria-label={labels.close} data-testid="tsumego-close" onClick={onClose}>
            ×
          </button>
        </header>
        {/* Java keeps the last good distance rather than refusing; skip the browser's range check. */}
        <form className="tsumego-form" noValidate onSubmit={submit}>
          <Choice name="to-play" label={labels.toPlay} values={TO_PLAY} names={labels.toPlayOptions} value={toPlay} onChange={setToPlay} />
          <Choice name="attacker" label={labels.attacker} values={ATTACKER} names={labels.attackerOptions} value={attacker} onChange={setAttacker} />
          <Choice name="ko" label={labels.koThreat} values={KO_THREAT} names={labels.koThreatOptions} value={koThreat} onChange={setKoThreat} />
          <div className="tsumego-row">
            <label htmlFor="tsumego-distance">{labels.wallDistance}</label>
            <div className="tsumego-distance">
              <input
                id="tsumego-distance"
                className="text-input"
                data-testid="tsumego-distance"
                type="number"
                min={1}
                step={1}
                inputMode="numeric"
                value={distance}
                onChange={(event) => setDistance(event.target.value)}
              />
              <span>{labels.wallDistanceHint}</span>
            </div>
          </div>
          {error ? (
            <div className="dialog-error" role="alert" data-testid="tsumego-error">
              <strong>{labels.failed}</strong>
              <pre>{error}</pre>
            </div>
          ) : null}
          <footer className="dialog-footer tsumego-footer">
            <button type="button" className="secondary-button" data-testid="tsumego-cancel" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="tsumego-confirm" disabled={busy}>
              {labels.confirm}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}

interface ChoiceProps<T extends string> {
  name: string;
  label: string;
  values: readonly T[];
  names: Record<T, string>;
  value: T;
  onChange(value: T): void;
}

function Choice<T extends string>({ name, label, values, names, value, onChange }: ChoiceProps<T>) {
  const id = `tsumego-${name}`;
  return (
    <div className="tsumego-row">
      <span id={id}>{label}</span>
      <div className="segmented tsumego-choice" role="radiogroup" aria-labelledby={id} style={{ gridTemplateColumns: `repeat(${values.length}, 1fr)` }}>
        {values.map((option) => (
          <button
            key={option}
            type="button"
            role="radio"
            aria-checked={value === option}
            data-testid={`${id}-${option.toLowerCase()}`}
            onClick={() => onChange(option)}
          >
            {names[option]}
          </button>
        ))}
      </div>
    </div>
  );
}

/** Command errors arrive as `{ type: "App", data: message }`. */
function describe(error: unknown): string {
  if (typeof error === "object" && error !== null && "data" in error && typeof error.data === "string") return error.data;
  return error instanceof Error ? error.message : String(error);
}
