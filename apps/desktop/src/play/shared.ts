import { useEffect } from "react";
import type { GtpEngineEntry } from "../api/gtp";
import type { MatchView } from "../api/match-game";
import type { EngineChoice } from "./AiFields";
import type { MatchLabels } from "./labels";
import { type AiForm, gtpTime, kataGoPlay } from "./setup";

const round = (value: number | null | undefined) => (value === null || value === undefined ? "–" : String(Math.round(value)));

/** Whether the AI fields describe a player the backend will accept. */
export function aiValid(form: AiForm, choices: readonly EngineChoice[]): boolean {
  const gtp = choices.find((choice) => choice.value === form.engine)?.gtp ?? false;
  return gtp ? gtpTime(form.gtp) !== null : kataGoPlay(form) !== null;
}

export function eloText(view: MatchView, labels: MatchLabels): string {
  const elo = view.elo;
  if (!elo || elo.elo === undefined) return labels.noElo;
  return labels.eloRange.replace("{elo}", round(elo.elo)).replace("{lower}", round(elo.lower)).replace("{upper}", round(elo.upper));
}

/** Java's title: "胜:负 A VS B 胜率 x%", counted from engine 1's side. */
export function matchTitle(view: MatchView, names: readonly [string, string], labels: MatchLabels): string {
  const a = view.a?.total ?? 0;
  const b = view.b?.total ?? 0;
  const draws = view.draws ?? 0;
  const played = a + b + draws;
  const rate = played > 0 ? Math.round(((a + draws / 2) / played) * 1000) / 10 : 0;
  return labels.titleScore.replace("{a}", String(a)).replace("{b}", String(b)).replace("{nameA}", names[0]).replace("{nameB}", names[1]).replace("{rate}", String(rate));
}

export function useEscape(onClose: () => void) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
}

export function engineChoices(katago: string, gtpEngines: readonly GtpEngineEntry[]): EngineChoice[] {
  return [{ value: "katago", label: katago, gtp: false }, ...gtpEngines.map((entry) => ({ value: entry.id ?? "", label: `${entry.name ?? entry.id} · GTP`, gtp: true }))];
}
