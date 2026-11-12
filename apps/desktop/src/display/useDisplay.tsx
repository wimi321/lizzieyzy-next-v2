import { useCallback, useState } from "react";
import type { Backend, DisplaySettings } from "@/api/backend";
import { type DisplayPrompt, displayBindings } from "./bindings";
import { NumberDialog, type NumberDialogLabels } from "./NumberDialog";
import { type Display, DISPLAY_DEFAULTS, filled } from "./settings";

export interface DisplayLabels extends NumberDialogLabels {
  numbersTitle: string;
  numbersPrompt: string;
  ranksTitle: string;
  ranksPrompt: string;
  minVisitsTitle: string;
  minVisitsPrompt: string;
}

/** Java's limits on the numbers asked for. */
const LIMITS: Record<DisplayPrompt, { min: number; max: number }> = {
  numbers: { min: 1, max: 999 },
  ranks: { min: 1, max: 999 },
  minVisits: { min: 0, max: 1_000_000 },
};

function report(error: unknown) {
  console.error("Display settings failed", error);
}

/**
 * The saved 显示 settings, the menu entries that change them, and the number prompts. A change
 * shows at once and is saved; if Rust refuses it, the saved settings are read back.
 */
export function useDisplay(backend: Pick<Backend, "setDisplaySettings" | "settings">, labels: DisplayLabels) {
  const [display, setDisplay] = useState<Display>(DISPLAY_DEFAULTS);
  const [prompt, setPrompt] = useState<DisplayPrompt | null>(null);

  const load = useCallback((settings: DisplaySettings | null | undefined) => setDisplay(filled(settings)), []);
  const set = (patch: Partial<Display>) => {
    const next = { ...display, ...patch };
    setDisplay(next);
    backend.setDisplaySettings(next).catch((error: unknown) => {
      report(error);
      backend
        .settings()
        .then((settings) => load(settings.display))
        .catch(report);
    });
  };

  const values: Record<DisplayPrompt, number> = {
    numbers: display.last_move_numbers,
    ranks: display.last_move_rank_marks,
    minVisits: display.next_move_min_visits,
  };
  const choose = (which: DisplayPrompt, value: number) => {
    setPrompt(null);
    if (which === "numbers") set({ move_numbers: "LastN", last_move_numbers: value });
    else if (which === "ranks") set({ move_rank_marks: "LastN", last_move_rank_marks: value });
    else set({ next_move_min_visits: value });
  };
  const dialog = prompt ? (
    <NumberDialog
      title={labels[`${prompt}Title`]}
      prompt={labels[`${prompt}Prompt`]}
      value={values[prompt]}
      min={LIMITS[prompt].min}
      max={LIMITS[prompt].max}
      labels={labels}
      testId={`display-${prompt}`}
      onConfirm={(value) => choose(prompt, value)}
      onClose={() => setPrompt(null)}
    />
  ) : null;

  return { display, load, set, bindings: displayBindings(display, set, setPrompt), dialog, dialogOpen: prompt !== null, values };
}
