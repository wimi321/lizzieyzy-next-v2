import type en from "../locales/en.json";

/** The 批量分析 and 精析 texts: one section of every locale file. */
export type BatchLabels = (typeof en)["batch"];

/** The whole-game presets' names, shared with 整盘精析. */
export type PresetLabels = Pick<(typeof en)["review"], "quick" | "standard" | "deep" | "professional" | "custom" | "perMove" | "customRange">;
