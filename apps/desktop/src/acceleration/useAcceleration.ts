import { useEffect, useRef, useState } from "react";
import type { AccelerationBackend, AccelerationPhase, AccelerationPlan, Backend, Settings } from "@/api/backend";
import type { ToolsTab } from "../engines/useEngineTools";
import { type Failure, defaultChoice, failureOf, hintGpu, optionsOf } from "./format";

type AccelerationApi = Pick<
  Backend,
  | "accelerationPlan"
  | "enableAcceleration"
  | "disableAcceleration"
  | "dismissAccelerationHint"
  | "subscribeAccelerationProgress"
  | "unsubscribe"
  | "cancelAssetDownload"
  | "pauseAssetDownload"
  | "startAssetDownload"
  | "settings"
>;

/** One enable in flight. Progress events only update it; the command's result ends it. */
export interface Run {
  backend: AccelerationBackend;
  phase: AccelerationPhase;
  started: number;
  tail: string[];
}

export type Outcome =
  | { type: "succeeded"; backend: AccelerationBackend }
  | { type: "restored" }
  | { type: "failed"; backend: AccelerationBackend; failure: Failure }
  | { type: "error"; failure: Failure };

interface Options {
  settings: Settings | null;
  /** 引擎一键设置 is open: read the plan for its 显卡提速 page. */
  toolsOpen: boolean;
  onSettings(settings: Settings): void;
  openTab(tab: ToolsTab): void;
}

/**
 * 显卡提速 (P3-31) over P3-30's commands. Lives with the main window, not the dialog, so an
 * enable that downloads for minutes keeps its state when the dialog is closed and reopened.
 */
export function useAcceleration(backend: AccelerationApi, { settings, toolsOpen, onSettings, openTab }: Options) {
  const [plan, setPlan] = useState<AccelerationPlan | null>(null);
  const [choice, setChoice] = useState<AccelerationBackend | null>(null);
  const [run, setRun] = useState<Run | null>(null);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [busy, setBusy] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [hint, setHint] = useState<string | null>(null);
  const hintChecked = useRef(false);

  const reload = () =>
    backend
      .accelerationPlan()
      .then(setPlan)
      .catch((error: unknown) => setOutcome({ type: "error", failure: failureOf(error) }));

  useEffect(() => {
    if (!toolsOpen) return;
    let active = true;
    backend
      .accelerationPlan()
      .then((next) => active && setPlan(next))
      .catch((error: unknown) => active && setOutcome({ type: "error", failure: failureOf(error) }));
    return () => {
      active = false;
    };
  }, [backend, toolsOpen]);

  useEffect(() => {
    if (!backend.subscribeAccelerationProgress) return;
    let active = true;
    let id: number | undefined;
    void backend
      .subscribeAccelerationProgress((progress) => {
        const { backend: from, phase } = progress;
        if (!active || !from || !phase || phase === "Ready" || phase === "Failed") return;
        setRun((current) => (current && current.backend === from ? { ...current, phase, tail: progress.output_tail ?? current.tail } : current));
      })
      .then((subscription) => {
        if (active) id = subscription;
        else void backend.unsubscribe(subscription);
      });
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend]);

  // The elapsed time shown while starting or tuning.
  useEffect(() => {
    if (!run) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [run]);

  // The one-time hint: read the plan once, only while the hint may still be shown.
  useEffect(() => {
    if (hintChecked.current || !settings || settings.acceleration_hint_dismissed) return;
    hintChecked.current = true;
    backend
      .accelerationPlan()
      .then((next) => setHint(hintGpu(next)))
      .catch(() => undefined);
  }, [backend, settings]);

  const options = optionsOf(plan);
  const selectable = (value: AccelerationBackend | null) => options.some((option) => option.backend === value && option.supported && value !== plan?.current);
  const selected = selectable(choice) ? choice : defaultChoice(plan);
  const profileOf = (value: AccelerationBackend) => options.find((option) => option.backend === value)?.profile_id;

  return {
    plan,
    options,
    selected,
    run,
    outcome,
    busy,
    elapsed: run ? Math.max(0, Math.round((now - run.started) / 1000)) : 0,
    hint,
    choose: (value: AccelerationBackend) => setChoice(value),
    enable: () => {
      const chosen = selected;
      if (!chosen || run || !backend.enableAcceleration) return;
      const started = Date.now();
      setNow(started);
      setOutcome(null);
      setRun({ backend: chosen, phase: "Downloading", started, tail: [] });
      backend
        .enableAcceleration(chosen)
        .then((next) => {
          onSettings(next);
          setOutcome({ type: "succeeded", backend: chosen });
        })
        .catch((error: unknown) => setOutcome({ type: "failed", backend: chosen, failure: failureOf(error) }))
        .finally(() => {
          setRun(null);
          void reload();
        });
    },
    /** Cancels the download, the start or the tuning; Rust then restores the previous engine. */
    cancel: () => void backend.cancelAssetDownload().catch(() => undefined),
    pause: () => void backend.pauseAssetDownload?.().catch(() => undefined),
    resume: () => {
      const profile = run ? profileOf(run.backend) : undefined;
      if (profile) void backend.startAssetDownload(profile).catch(() => undefined);
    },
    disable: () => {
      if (!backend.disableAcceleration || run) return;
      setBusy(true);
      setOutcome(null);
      backend
        .disableAcceleration()
        .then(() => backend.settings())
        .then((next) => {
          onSettings(next);
          setOutcome({ type: "restored" });
        })
        .catch((error: unknown) => setOutcome({ type: "error", failure: failureOf(error) }))
        .finally(() => {
          setBusy(false);
          void reload();
        });
    },
    openBenchmark: () => openTab("benchmark"),
    openFromHint: () => {
      setHint(null);
      openTab("acceleration");
    },
    /** 不再提示, or closing the hint: saved, never shown again. */
    dismissHint: () => {
      setHint(null);
      void backend.dismissAccelerationHint?.().catch(() => undefined);
    },
  };
}

export type Acceleration = ReturnType<typeof useAcceleration>;
