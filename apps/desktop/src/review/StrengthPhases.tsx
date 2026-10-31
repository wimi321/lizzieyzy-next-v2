import { type KeyboardEvent, type PointerEvent, useRef, useState } from "react";
import type { MoveCategory, PhaseEstimate, Sample, SideReport, StrengthPhasesReport } from "@/api/backend";
import { DEFAULT_BOUNDS, MIN_PHASE_MOVES, reviewLine, verdict } from "./phases";
import type { StrengthLabels } from "./StrengthDialog";
import { percent } from "./strength";

/** Score loss, in points, that fills half the strip's height. */
const STRIP_LOSS = 6;

const CATEGORY_CLASS: Record<MoveCategory, string> = {
  Excellent: "best",
  Great: "good",
  Good: "normal",
  Inaccuracy: "inaccuracy",
  Mistake: "mistake",
  Blunder: "blunder",
};

export interface StrengthPhasesProps {
  report: StrengthPhasesReport;
  /** The boundaries the user dragged to, or null for Java's defaults. */
  requested: readonly [number, number] | null;
  lastMove: number;
  /** Every analysed move of the game, for the strip under the dividers. */
  samples: readonly Sample[];
  names: { black: string; white: string };
  chinese: boolean;
  labels: StrengthLabels;
  onBounds(bounds: [number, number] | null): void;
}

type Side = "black" | "white";

/** LizzieYzy Next's 分阶段 page: the game in three phases, each side's level in each. */
export function StrengthPhases({ report, requested, lastMove, samples, names, chinese, labels, onBounds }: StrengthPhasesProps) {
  const bounds = requested ?? report.bounds;
  const phaseNames = [labels.phaseNames.opening, labels.phaseNames.middlegame, labels.phaseNames.endgame] as const;
  const review = reviewLine(report, names, chinese ? phaseNames : phaseNames.map((name) => name.toLowerCase()), labels.phase);
  return (
    <section className="phase-view" data-testid="strength-phases" aria-label={labels.phase.title}>
      <header className="phase-view-header">
        <h3>{labels.phase.title}</h3>
        <p>{labels.phase.hint}</p>
        {requested ? (
          <button type="button" className="phase-reset" data-testid="phase-reset" onClick={() => onBounds(null)}>
            {labels.phase.reset}
          </button>
        ) : null}
      </header>
      <Timeline bounds={bounds} lastMove={lastMove} samples={samples} names={phaseNames} labels={labels} onBounds={onBounds} />
      <div className="phase-cards">
        {report.phases.map((phase, index) => (
          <PhaseCard
            key={phase.index}
            phase={phase}
            name={phaseNames[index] ?? ""}
            badges={{ black: badge(report.black, index), white: badge(report.white, index) }}
            names={names}
            chinese={chinese}
            labels={labels}
          />
        ))}
      </div>
      {review ? (
        <p className="phase-review" data-testid="phase-review">
          {review}
        </p>
      ) : null}
      <p className="phase-caveat">{labels.phase.caveat}</p>
    </section>
  );
}

function badge(extremes: StrengthPhasesReport["black"], index: number): "best" | "weakest" | null {
  if (extremes.best === index) return "best";
  if (extremes.worst === index) return "weakest";
  return null;
}

interface TimelineProps {
  bounds: readonly [number, number];
  lastMove: number;
  samples: readonly Sample[];
  names: readonly string[];
  labels: StrengthLabels;
  onBounds(bounds: [number, number]): void;
}

/** Java's `StrengthPhaseTimeline`: the game as a bar, its two dividers dragged or moved by keys. */
function Timeline({ bounds, lastMove, samples, names, labels, onBounds }: TimelineProps) {
  const bar = useRef<HTMLDivElement>(null);
  // The axis holds still while a divider is dragged, as in Java.
  const [frozenEnd, setFrozenEnd] = useState<number | null>(null);
  const axisEnd = frozenEnd ?? Math.max(lastMove, Math.max(DEFAULT_BOUNDS[1], bounds[1]) + MIN_PHASE_MOVES);
  const at = (move: number) => `${(Math.max(0, Math.min(move, axisEnd)) / axisEnd) * 100}%`;
  const span = (from: number, to: number) => ({ left: at(from), width: `${(Math.max(0, Math.min(to, axisEnd) - from) / axisEnd) * 100}%` });

  const limits = (index: 0 | 1): [number, number] => {
    if (index === 0) return [MIN_PHASE_MOVES, bounds[1] - MIN_PHASE_MOVES];
    const low = bounds[0] + MIN_PHASE_MOVES;
    return [low, Math.max(low, axisEnd - MIN_PHASE_MOVES)];
  };
  const move = (index: 0 | 1, target: number) => {
    const [low, high] = limits(index);
    const value = Math.max(low, Math.min(high, Math.round(target)));
    if (value !== bounds[index]) onBounds(index === 0 ? [value, bounds[1]] : [bounds[0], value]);
  };
  const drag = (index: 0 | 1, event: PointerEvent<HTMLDivElement>) => {
    const box = bar.current?.getBoundingClientRect();
    if (!box || box.width === 0 || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
    move(index, ((event.clientX - box.left) / box.width) * axisEnd);
  };
  const key = (index: 0 | 1, event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 10 : 1;
    const [low, high] = limits(index);
    const target =
      event.key === "ArrowLeft" || event.key === "ArrowDown"
        ? bounds[index] - step
        : event.key === "ArrowRight" || event.key === "ArrowUp"
          ? bounds[index] + step
          : event.key === "Home"
            ? low
            : event.key === "End"
              ? high
              : null;
    if (target === null) return;
    event.preventDefault();
    move(index, target);
  };

  const extents: [number, number][] = [
    [0, bounds[0]],
    [bounds[0], bounds[1]],
    [bounds[1], axisEnd],
  ];
  const label = (index: number) => {
    const first = index === 0 ? 1 : (bounds[index - 1] ?? 0) + 1;
    const last = index === 2 ? lastMove : Math.min(bounds[index] ?? 0, lastMove);
    return last < first ? labels.unplayed : `${first}–${last}`;
  };
  const step = axisEnd > 150 ? 50 : 20;
  const ticks = Array.from({ length: Math.floor(axisEnd / step) }, (_, index) => (index + 1) * step).filter((move) => move < axisEnd);

  return (
    <div className="phase-timeline" data-testid="phase-timeline">
      <div className="phase-labels" aria-hidden="true">
        {extents.map(([from, to], index) => (
          <span key={index} className="phase-label" style={span(from, to)}>
            <b>{names[index]}</b> · {label(index)}
          </span>
        ))}
      </div>
      <div className="phase-track">
        <span className="phase-key" aria-hidden="true">
          <span className="stone-icon black" />
          <span className="stone-icon white" />
        </span>
        <div className="phase-bar" ref={bar}>
          {extents.map(([from, to], index) => (
            <span key={index} className={`phase-band p${index}`} style={span(from, to)} />
          ))}
          {lastMove < axisEnd ? <span className="phase-band unreached" style={span(lastMove, axisEnd)} /> : null}
          <svg className="phase-strip" viewBox={`0 0 ${axisEnd} 40`} preserveAspectRatio="none" aria-hidden="true">
            <line className="mid" x1={0} x2={axisEnd} y1={20} y2={20} />
            {samples.map((sample) => {
              const height = Math.max(0.12, Math.min(1, sample.score_equivalent_loss / STRIP_LOSS)) * 18;
              return (
                <rect
                  key={`${sample.move_number}-${sample.is_black ? "b" : "w"}`}
                  className={CATEGORY_CLASS[sample.category]}
                  x={sample.move_number - 0.85}
                  width={0.7}
                  y={sample.is_black ? 20 - height : 20}
                  height={height}
                />
              );
            })}
          </svg>
          {([0, 1] as const).map((index) => (
            <div
              key={index}
              className="phase-grip"
              role="slider"
              tabIndex={0}
              data-testid={`phase-divider-${index}`}
              style={{ left: at(bounds[index]) }}
              aria-label={labels.phase.divider
                .replace("{a}", names[index] ?? "")
                .replace("{b}", names[index + 1] ?? "")
                .replace("{n}", String(bounds[index]))}
              aria-valuemin={limits(index)[0]}
              aria-valuemax={limits(index)[1]}
              aria-valuenow={bounds[index]}
              onPointerDown={(event) => {
                event.preventDefault();
                event.currentTarget.setPointerCapture(event.pointerId);
                event.currentTarget.focus();
                setFrozenEnd(axisEnd);
              }}
              onPointerMove={(event) => drag(index, event)}
              onPointerUp={(event) => {
                event.currentTarget.releasePointerCapture(event.pointerId);
                setFrozenEnd(null);
              }}
              onPointerCancel={() => setFrozenEnd(null)}
              onKeyDown={(event) => key(index, event)}
            />
          ))}
        </div>
      </div>
      <div className="phase-axis" aria-hidden="true">
        {ticks.map((move) => (
          <span key={move} style={{ left: at(move) }}>
            {move}
          </span>
        ))}
      </div>
    </div>
  );
}

interface PhaseCardProps {
  phase: PhaseEstimate;
  name: string;
  badges: Record<Side, "best" | "weakest" | null>;
  names: { black: string; white: string };
  chinese: boolean;
  labels: StrengthLabels;
}

function PhaseCard({ phase, name, badges, names, chinese, labels }: PhaseCardProps) {
  return (
    <article className={`phase-card p${phase.index}`} data-testid={`phase-card-${phase.index}`} data-played={phase.played}>
      <header>
        <span className="phase-card-name">{name}</span>
        <span className="phase-card-range">
          {phase.played ? labels.phaseRange.replace("{a}", String(phase.first)).replace("{b}", String(phase.last)) : labels.unplayed}
        </span>
      </header>
      {phase.played ? (
        <>
          {(["black", "white"] as const).map((side) => (
            <SideLine
              key={side}
              side={side}
              name={names[side]}
              report={phase[side]}
              ranked={phase[`${side}_ranked`]}
              badge={badges[side]}
              testId={`phase-${phase.index}-${side}`}
              chinese={chinese}
              labels={labels}
            />
          ))}
          <p className="phase-verdict" data-testid={`phase-verdict-${phase.index}`}>
            {verdict(phase, names, labels.phase)}
          </p>
        </>
      ) : (
        <p className="phase-unreached">{labels.phase.notReached}</p>
      )}
    </article>
  );
}

interface SideLineProps {
  side: Side;
  name: string;
  report: SideReport;
  ranked: boolean;
  badge: "best" | "weakest" | null;
  testId: string;
  chinese: boolean;
  labels: StrengthLabels;
}

function SideLine({ side, name, report, ranked, badge, testId, chinese, labels }: SideLineProps) {
  const loss = report.average_score_loss;
  const stats = [labels.moves.replace("{n}", String(report.sample_count))];
  if (ranked) stats.push(`${labels.match} ${percent(report.match_rate)}`);
  // The line holds two figures at card width; the average loss waits in the tooltip.
  const title = ranked && loss !== null ? `${labels.avgLossShort} ${labels.points.replace("{n}", loss.toFixed(2))}` : undefined;
  return (
    <div className="phase-side" data-testid={testId}>
      <div className="phase-side-line">
        <span className={`stone-icon ${side}`} aria-hidden="true" />
        <span className="phase-player">{name}</span>
        <strong className="phase-rank" data-testid={`${testId}-rank`}>
          {ranked ? (chinese ? report.rank_label_zh : report.rank_label_en) : "—"}
        </strong>
        {badge ? <span className={`phase-badge ${badge}`}>{labels.phase[badge]}</span> : <span />}
      </div>
      <p className="phase-side-stats" title={title}>
        {stats.join(" · ")}
      </p>
    </div>
  );
}
