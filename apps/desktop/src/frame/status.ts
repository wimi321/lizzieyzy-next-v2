import type { EngineStatus } from "@/api/backend";
import { joinParts } from "./title";

export interface StatusLabels {
  noEngine: string;
  analyzing: string;
  paused: string;
  autoAnalyzing: string;
  spaceToPause: string;
  spaceToResume: string;
}

export interface StatusInput {
  engine: EngineStatus;
  /** The engine's name; null when none is configured. */
  name: string | null;
  configured: boolean;
  autoAnalyzing: boolean;
  pondering: boolean;
  labels: StatusLabels;
  states: Readonly<Record<EngineStatus["type"], string>>;
}

/** Java's status line, "engine · state · the key to press", and what a hover adds: the failure or the version. */
export function statusLine({ engine, name, configured, autoAnalyzing, pondering, labels, states }: StatusInput): { text: string; detail: string } {
  const ready = engine.type === "Ready";
  const text = joinParts(
    " · ",
    configured ? name : labels.noEngine,
    !ready ? states[engine.type] : autoAnalyzing ? labels.autoAnalyzing : pondering ? labels.analyzing : labels.paused,
    ready && !autoAnalyzing ? (pondering ? labels.spaceToPause : labels.spaceToResume) : null,
  );
  const detail =
    engine.type === "Failed" || engine.type === "Restarting" ? engine.data.reason : engine.type === "Ready" ? engine.data.version : states[engine.type];
  return { text, detail };
}
