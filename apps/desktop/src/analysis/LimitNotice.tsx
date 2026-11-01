import type { AnalysisLimitSettings, AnalysisPauseReason } from "@/api/backend";

export interface LimitNoticeLabels {
  title: string;
  time: string;
  playouts: string;
  minutes: string;
  seconds: string;
  resume: string;
  change: string;
  dismiss: string;
}

function duration(seconds: number, labels: LimitNoticeLabels): string {
  return seconds % 60 === 0 ? labels.minutes.replace("{n}", String(seconds / 60)) : labels.seconds.replace("{n}", String(seconds));
}

/**
 * Over the board when live analysis stopped at its limit, as LizzieYzy Next tells: why it
 * stopped, that Space resumes, and where the limits are set.
 */
export function LimitNotice(props: {
  reason: Extract<AnalysisPauseReason, "TimeLimit" | "PlayoutLimit">;
  limits: AnalysisLimitSettings | null;
  labels: LimitNoticeLabels;
  onResume(): void;
  onChange(): void;
  onDismiss(): void;
}) {
  const { reason, limits, labels } = props;
  const detail =
    reason === "TimeLimit"
      ? labels.time.replace("{n}", limits ? duration(limits.max_analyze_time_seconds, labels) : "")
      : labels.playouts.replace("{n}", limits ? limits.limit_playouts.toLocaleString() : "");
  return (
    <div className="limit-notice" role="status" data-testid="limit-notice" data-reason={reason}>
      <span className="limit-notice-dot" aria-hidden="true" />
      <strong>{labels.title}</strong>
      <span className="limit-notice-detail">{detail}</span>
      <button type="button" className="limit-notice-action primary" data-testid="limit-resume" onClick={props.onResume}>
        {labels.resume}
      </button>
      <button type="button" className="limit-notice-action" data-testid="limit-change" onClick={props.onChange}>
        {labels.change}
      </button>
      <button type="button" className="limit-notice-close" aria-label={labels.dismiss} title={labels.dismiss} data-testid="limit-dismiss" onClick={props.onDismiss}>
        ×
      </button>
    </div>
  );
}
