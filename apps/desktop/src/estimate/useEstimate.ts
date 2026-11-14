import { useEffect, useRef, useState } from "react";
import type { AnalysisView, Backend, EstimateLocation } from "@/api/backend";
import type { Estimate } from "../board/estimate";
import type { Display } from "../display/settings";
import type { MenuBinding } from "../menu/bindings";

const onMain = (location: EstimateLocation) => location === "Main" || location === "Both";
const onSub = (location: EstimateLocation) => location === "Sub" || location === "Both";

function report(error: unknown) {
  console.error("Estimate command failed", error);
}

/**
 * Kata形势, 纯网络 and 策略网络: what each board draws, the menu entries that are not plain
 * display settings, and the extra data KataGo is asked for only while something shows it.
 */
export function useEstimate(
  backend: Pick<Backend, "setAnalysisExtras" | "setPureNetwork">,
  display: Display,
  set: (patch: Partial<Display>) => void,
  analysis: AnalysisView | null,
) {
  // Neither is saved, as in Java: both start off.
  const [pure, setPure] = useState(false);
  const [policyShown, setPolicyShown] = useState(false);
  const lastOn = useRef<EstimateLocation>("Both");
  const location = display.estimate_location;
  useEffect(() => {
    if (location !== "Off" && location !== "Hidden") lastOn.current = location;
  }, [location]);

  // Ask only for what is drawn: the policy for 策略网络, each move's ownership for 按选点显示评估.
  const policy = policyShown && !pure;
  const movesOwnership = display.moves_ownership && onMain(location) && !pure;
  useEffect(() => {
    backend.setAnalysisExtras({ policy, moves_ownership: movesOwnership }).catch(report);
  }, [backend, policy, movesOwnership]);

  const ownership = analysis?.ownership ?? null;
  const mainEstimate: Estimate | null =
    ownership && onMain(location) ? { style: pure ? display.pure_estimate_style : display.estimate_style, values: ownership } : null;
  const subEstimate: Estimate | null = ownership && onSub(location) ? { style: display.estimate_style, values: ownership } : null;
  const heatKind = display.sub_board_heat;

  const togglePure = () => {
    const next = !pure;
    setPure(next);
    backend.setPureNetwork(next).catch((error: unknown) => {
      setPure(!next);
      report(error);
    });
  };
  // The bottom bar's Kata形势 and the "." key: off, or back to where it was last shown.
  const toggle = () => set({ estimate_location: location === "Off" || location === "Hidden" ? lastOn.current : "Off" });

  const bindings: Record<string, MenuBinding> = {
    showHeatmap: { run: togglePure, checked: pure },
    showPolicy: { run: () => setPolicyShown((shown) => !shown), checked: policyShown },
    toggleKataEstimate: { run: toggle, enabled: display.estimate_shortcut },
  };

  return {
    on: onMain(location) || onSub(location),
    toggle,
    pure,
    bindings,
    board: {
      estimate: mainEstimate,
      movesOwnership,
      policy: pure || policyShown ? (analysis?.policy ?? null) : null,
      pure,
    },
    subBoard: {
      estimate: subEstimate,
      heat: heatKind !== "None" && analysis ? { kind: heatKind, candidates: analysis.candidates } : null,
      raw: display.sub_board_raw,
    },
  };
}
