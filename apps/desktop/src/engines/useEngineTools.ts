import { useEffect, useState } from "react";
import type { AccelerationPlan, AssetDownloadStatus, Backend, DiagnosticRepair, EngineDiagnostics, OfficialWeight, Settings } from "@/api/backend";
import { describe } from "./model";

type ToolsBackend = Pick<
  Backend,
  | "officialWeights"
  | "startAssetDownload"
  | "cancelAssetDownload"
  | "assetDownloadStatus"
  | "subscribeAssetDownloadProgress"
  | "unsubscribe"
  | "accelerationPlan"
  | "useOfficialWeight"
  | "diagnoseEngine"
  | "repairEngine"
>;

export type ToolsTab = "overview" | "weights" | "acceleration" | "benchmark";

/**
 * What 引擎一键设置's tabs show (P2-12): the official weights and the download in progress, the
 * NVIDIA plan, and the main engine's diagnostics. Read while the dialog is open; the download
 * keeps reporting while it runs, whatever tab is shown.
 */
export function useEngineTools(backend: ToolsBackend, open: boolean, onSettings: (settings: Settings) => void) {
  const [weights, setWeights] = useState<OfficialWeight[] | null>(null);
  const [download, setDownload] = useState<AssetDownloadStatus | null>(null);
  const [plan, setPlan] = useState<AccelerationPlan | null>(null);
  const [diagnostics, setDiagnostics] = useState<EngineDiagnostics | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Subscribe before reading the status, so no progress falls between the two.
  useEffect(() => {
    if (!open) return;
    let active = true;
    let id: number | undefined;
    backend
      .subscribeAssetDownloadProgress((status) => {
        if (!active) return;
        setDownload(status);
        // A finished download changes what is on disk.
        if (status.phase === "Completed") void backend.officialWeights().then((list) => active && setWeights(list));
      })
      .then((subscription) => {
        if (active) id = subscription;
        else void backend.unsubscribe(subscription);
        return backend.assetDownloadStatus();
      })
      .then((status) => active && setDownload(status))
      .catch((reason: unknown) => active && setError(describe(reason)));
    Promise.all([backend.officialWeights(), backend.accelerationPlan(), backend.diagnoseEngine()])
      .then(([list, nextPlan, report]) => {
        if (!active) return;
        setWeights(list);
        setPlan(nextPlan);
        setDiagnostics(report);
      })
      .catch((reason: unknown) => active && setError(describe(reason)));
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend, open]);

  const act = <T>(action: () => Promise<T>, then: (value: T) => void) => {
    setBusy(true);
    setError(null);
    action()
      .then(then)
      .catch((reason: unknown) => setError(describe(reason)))
      .finally(() => setBusy(false));
  };

  return {
    weights,
    download,
    plan,
    diagnostics,
    busy,
    error,
    downloading: download?.phase === "Downloading" || download?.phase === "Installing",
    startDownload: (id: string) => act(() => backend.startAssetDownload(id), setDownload),
    cancelDownload: () => act(() => backend.cancelAssetDownload(), setDownload),
    chooseWeight: (id: string) =>
      act(
        () => backend.useOfficialWeight(id),
        (settings) => {
          onSettings(settings);
          void backend.diagnoseEngine().then(setDiagnostics);
        },
      ),
    diagnose: () => act(() => backend.diagnoseEngine(), setDiagnostics),
    repair: (repair: DiagnosticRepair) => act(() => backend.repairEngine(repair), setDiagnostics),
  };
}

export type EngineTools = ReturnType<typeof useEngineTools>;
