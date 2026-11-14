import { useEffect } from "react";
import type { AnalysisView, Backend, GameInfoView } from "@/api/backend";
import { evaluationText, windowTitle } from "./title";

export interface WindowTitleLabels {
  black: string;
  white: string;
  result: string;
  speedUnit: string;
  b11: string;
  trial: string;
}

export interface WindowTitleInput {
  app: string;
  analysis: AnalysisView | null;
  /** The node on the board; analysis of another node is stale. */
  current: number | null;
  engine: string | null;
  pondering: boolean;
  info: GameInfoView | null;
  file: string | null;
  trial: boolean;
  labels: WindowTitleLabels;
  /** An engine match's score, which Java puts first in the title. */
  match?: string | null;
}

/** Java's title, "[Black's winrate, lead, visits] engine speed - [黑] A vs [白] B (结果:…)", or 试下中... during a trial. */
export function titleOf({ app, analysis, current, engine, pondering, info, file, trial, labels, match }: WindowTitleInput): string {
  if (trial) return labels.trial;
  const speed = analysis?.visits_per_second ?? null;
  const title = windowTitle({
    app,
    evaluation:
      analysis && analysis.node_id === current && analysis.visits > 0 ? evaluationText(analysis.winrate, analysis.score_lead, analysis.visits) : null,
    engine,
    speed:
      (pondering && speed !== null ? `${Math.round(speed)} ${labels.speedUnit}` : "") +
      (engine !== null && /b11|11b/i.test(engine) ? ` · ${labels.b11} ` : ""),
    players: info && (info.black_name || info.white_name) ? `[${labels.black}] ${info.black_name ?? ""} vs [${labels.white}] ${info.white_name ?? ""}` : null,
    result: info?.result?.trim() ? `(${labels.result}${info.result.trim()})` : null,
    file,
  });
  return match ? `${match} - ${title}` : title;
}

/** Keeps the native window's title in step with the game. */
export function useWindowTitle(backend: Pick<Backend, "setWindowTitle">, input: WindowTitleInput): void {
  const title = titleOf(input);
  useEffect(() => {
    backend.setWindowTitle(title).catch((error: unknown) => console.error("Window title failed", error));
  }, [backend, title]);
}
