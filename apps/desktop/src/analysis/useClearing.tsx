import { useState } from "react";
import type { Backend } from "@/api/backend";
import { ConfirmDialog } from "../frame/ConfirmDialog";
import type { MenuBinding } from "../menu/bindings";

export interface ClearingLabels {
  analysisTitle: string;
  analysisAll: string;
  cacheTitle: string;
  cacheAll: string;
  confirm: string;
  cancel: string;
}

type Whole = "analysis" | "cache";

function report(error: unknown) {
  console.error("Clearing failed", error);
}

/**
 * 分析 → 清除分析信息 and 清除Lizzie缓存, this move or all. Clearing the game's analysis is an
 * edit that 撤销编辑 undoes; clearing the cache only touches the database. The whole-game and
 * whole-cache entries ask first.
 */
export function useClearing(backend: Pick<Backend, "clearAnalysis" | "clearCache">, hasBoard: boolean, labels: ClearingLabels) {
  const [asking, setAsking] = useState<Whole | null>(null);
  const clear = (what: Whole, all: boolean) => {
    const run = what === "analysis" ? backend.clearAnalysis?.(all) : backend.clearCache?.(all);
    run?.catch(report);
  };
  const bindings: Record<string, MenuBinding> = {
    clearThisLizzieBestmoves: { run: () => clear("analysis", false), enabled: hasBoard },
    clearAllLizzieBestmoves: { run: () => setAsking("analysis"), enabled: hasBoard },
    clearThisLizzieCache: { run: () => clear("cache", false), enabled: hasBoard },
    clearAllLizzieCache: { run: () => setAsking("cache"), enabled: true },
  };
  const dialog = asking ? (
    <ConfirmDialog
      title={asking === "analysis" ? labels.analysisTitle : labels.cacheTitle}
      lines={[asking === "analysis" ? labels.analysisAll : labels.cacheAll]}
      confirm={labels.confirm}
      cancel={labels.cancel}
      testId={`confirm-clear-${asking}`}
      onConfirm={() => {
        clear(asking, true);
        setAsking(null);
      }}
      onCancel={() => setAsking(null)}
    />
  ) : null;
  return { bindings, dialog, dialogOpen: asking !== null };
}
