import type { EngineEntry, EngineProfile, EngineRules, EngineSettings, Settings } from "@/api/backend";

/** A configured engine; Rust always sends all three fields. */
export interface Engine {
  id: string;
  name: string;
  profile: EngineProfile;
}

export type EngineOptions = Required<EngineSettings>;

/** Java's defaults (P2-11): follow the game's rules, read its komi, watch the engine, report every 100 ms. */
export const ENGINE_DEFAULTS: EngineOptions = {
  rules: { type: "FollowSgf" },
  read_komi: true,
  default_komi: 7.5,
  auto_check_alive: true,
  report_interval_ms: 100,
};

/** Java's 引擎规则 choices, in its order; a custom rule set stays as the backend sent it. */
export const RULES: readonly EngineRules["type"][] = [
  "FollowSgf",
  "Chinese",
  "Japanese",
  "Korean",
  "Aga",
  "TrompTaylor",
  "NewZealand",
  "ChineseOgs",
  "ChineseKgs",
  "StoneScoring",
  "Bga",
  "AgaButton",
  "Goe",
  "AncientTerritory",
];

function complete(entry: EngineEntry): entry is Engine {
  return typeof entry.id === "string" && typeof entry.name === "string" && entry.profile !== undefined;
}

export interface EngineState {
  engines: Engine[];
  main: string | null;
  options: EngineOptions;
}

export function engineState(settings: Settings): EngineState {
  return {
    engines: (settings.engines ?? []).filter(complete),
    main: settings.main_engine_id ?? null,
    options: { ...ENGINE_DEFAULTS, ...settings.engine_settings },
  };
}

/**
 * How an engine is named in the menu and the list. An engine migrated from the single-engine
 * settings is called "KataGo"; while it is the main engine, the name Rust gives its model reads better.
 */
export function engineName(engine: Engine, isMain: boolean, mainLabel: string | null): string {
  return isMain && mainLabel && engine.name === "KataGo" ? mainLabel : engine.name;
}

/** A new engine's name: its model file without the extensions, as a starting point. */
export function nameFromModel(model: string): string {
  const file = model.split(/[\\/]/).pop() ?? model;
  return file.replace(/\.(bin|txt)(\.gz)?$/i, "") || "KataGo";
}

/** What went wrong, from a typed backend error or anything else. */
export function describe(error: unknown): string {
  if (typeof error === "object" && error !== null && "data" in error) {
    const data = (error as { data: unknown }).data;
    if (typeof data === "string") return data;
    if (typeof data === "object" && data !== null && "reason" in data) return String((data as { reason: unknown }).reason);
  }
  return error instanceof Error ? error.message : String(error);
}
