import { useEffect, useState } from "react";
import type { AnalysisView, Backend, BoardView, ComparisonView, DocumentView, LayoutMode, PointView, Settings } from "@/api/backend";
import type { Estimate } from "../board/estimate";
import { describe } from "../engines/model";
import type { MenuBinding, MenuBindings } from "../menu/bindings";
import type { ComparisonMenus } from "../menu/useMenus";
import { ComparisonBoard, type ComparisonLabels, EngineLines } from "./ComparisonBoard";

export type ComparisonBackend = Pick<
  Backend,
  | "comparisonState"
  | "selectComparisonEngine"
  | "restartComparisonEngine"
  | "closeComparisonEngine"
  | "closeOtherEngines"
  | "closeAllEngines"
  | "subscribeComparisonChanged"
  | "subscribeComparisonAnalysisUpdated"
  | "unsubscribe"
  | "previewVariation"
>;

/** What the engine list gives: its entries, the main engine, their menu names and its bindings. */
export interface ComparisonEngines {
  engines: readonly { id: string }[];
  main: string | null;
  menuNames: readonly string[];
  bindings: MenuBindings;
  /** Settings a command returned. */
  apply(settings: Settings): void;
}

interface Options {
  engines: ComparisonEngines;
  document: DocumentView | null;
  mode: LayoutMode;
  /** 双引擎模式 as the layout menu chooses it, with its window size. */
  enter(): void;
}

const STOPPED: ComparisonView = { engine_id: null, status: { type: "Stopped" }, analysis: null };
const TOAST_MS = 4000;
const fill = (text: string, values: Record<string, string>) => text.replace(/\{(\w+)\}/g, (whole, key: string) => values[key] ?? whole);

/** Rust's refusal in words: the same-engine rule has its own, Java's, wording. */
export function comparisonError(error: unknown, labels: ComparisonLabels): string {
  const type = typeof error === "object" && error !== null && "type" in error ? (error as { type: unknown }).type : null;
  if (type === "SameAsMain" || type === "SameAsComparison") return labels.sameEngine;
  return fill(labels.failed, { reason: describe(error) });
}

/** The strip's 双引擎 menu opens as if clicked: 搜索功能's 选择对比引擎 leads there, as in Java. */
function openComparisonMenu() {
  window.document.querySelector<HTMLElement>('[data-testid="menubar"] [data-menu-id="engineMenu2"]')?.click();
}

/**
 * 对比引擎 (P3-37) over P3-36: the second engine menu and the close entries, 搜索功能's 选择对比引擎,
 * 双引擎模式's right board and status lines, and the prompt for a game that carries LZ2.
 */
export function useComparison(backend: ComparisonBackend, labels: ComparisonLabels, { engines, document, mode, enter }: Options) {
  const [view, setView] = useState<ComparisonView>(STOPPED);
  const [analysis, setAnalysis] = useState<AnalysisView | null>(null);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    const ids: number[] = [];
    const keep = (id: number) => (active ? ids.push(id) : void backend.unsubscribe(id));
    void backend
      .subscribeComparisonChanged?.((next) => {
        if (!active) return;
        setView(next);
        setAnalysis(next.analysis);
      })
      .then(keep);
    void backend.subscribeComparisonAnalysisUpdated?.((next) => active && setAnalysis(next)).then(keep);
    void backend
      .comparisonState?.()
      .then((next) => {
        if (!active) return;
        setView(next);
        setAnalysis(next.analysis);
      })
      .catch(() => undefined);
    return () => {
      active = false;
      for (const id of ids) void backend.unsubscribe(id);
    };
  }, [backend]);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), TOAST_MS);
    return () => clearTimeout(timer);
  }, [toast]);

  const double = mode === "DoubleEngine";
  const chosen = view.engine_id;
  const index = engines.engines.findIndex((engine) => engine.id === chosen);
  const name = index >= 0 ? (engines.menuNames[index] ?? null) : null;

  const act = (command: (() => Promise<Settings>) | undefined) => {
    if (!command) return;
    setBusy(true);
    command()
      .then(engines.apply)
      .catch((error: unknown) => setToast(comparisonError(error, labels)))
      .finally(() => setBusy(false));
  };

  const bindings: Record<string, MenuBinding> = {
    restartCurrentEngine2: { run: () => act(backend.restartComparisonEngine), enabled: chosen !== null && !busy },
    shutdownCurrentEngine2: { run: () => act(backend.closeComparisonEngine), enabled: chosen !== null && !busy },
    shutdownOtherEngine: { run: () => act(backend.closeOtherEngines), enabled: chosen !== null && !busy },
    shutdownAllEngine: { run: () => act(backend.closeAllEngines), enabled: (chosen !== null || engines.main !== null) && !busy },
    selectComparisonEngine: { run: openComparisonMenu, enabled: double, ...(double ? {} : { reason: labels.unavailable }) },
  };
  engines.engines.forEach((engine, at) => {
    const select = backend.selectComparisonEngine;
    bindings[`comparisonEngine${at}`] = {
      run: () => act(select && (() => select(engine.id))),
      enabled: engine.id !== engines.main && !busy,
      checked: engine.id === chosen,
    };
    // Java: 第一引擎不能与第二引擎相同, so the main engine menu cannot pick the comparison engine.
    const main = engines.bindings[`engine${at}`];
    if (engine.id === chosen && main) bindings[`engine${at}`] = { ...main, enabled: false };
  });

  const menus: ComparisonMenus = {
    shown: double,
    selected: index >= 0 ? index : null,
    search: { text: labels.search, keywords: labels.keywords.split("|") },
  };

  const key = document ? `${document.file_path ?? ""}\n${document.file_name ?? ""}` : null;
  const prompt = !double && document?.has_comparison_analysis === true && key !== dismissed;
  const chip = (
    <>
      {prompt ? (
        <span className="chip comparison-prompt" data-testid="comparison-prompt">
          <span className="chip-label">{labels.prompt}</span>
          <button
            type="button"
            className="secondary-button"
            data-testid="comparison-enter"
            onClick={() => {
              setDismissed(key);
              enter();
            }}
          >
            {labels.enter}
          </button>
          <button type="button" className="dialog-close" aria-label={labels.close} data-testid="comparison-dismiss" onClick={() => setDismissed(key)}>
            ×
          </button>
        </span>
      ) : null}
      {toast ? (
        <span className="web-board-toast" role="status" data-testid="comparison-toast">
          {toast}
        </span>
      ) : null}
    </>
  );

  const current = (node: number | null) => (analysis && analysis.node_id === node ? analysis : null);

  return {
    bindings,
    menus,
    chip,
    /** 双引擎模式's right board; undefined in the other layouts. */
    board: (props: { board: BoardView; node: number | null; revision: number; coordinates: boolean; estimate: Estimate | null; maxCandidates: number; onPlay(point: PointView): void }) =>
      double ? <ComparisonBoard backend={backend} analysis={current(props.node)} unchosen={chosen === null} labels={labels} {...props} /> : undefined,
    /** 双引擎模式's status lines under both boards; undefined in the other layouts. */
    lines: (main: { name: string | null; analysis: AnalysisView | null; node: number | null; pondering: boolean }) =>
      double ? (
        <EngineLines
          main={{ name: main.name, analysis: main.analysis }}
          comparison={{ name, analysis: current(main.node), status: view.status }}
          pondering={main.pondering}
          labels={labels}
        />
      ) : undefined,
  };
}
