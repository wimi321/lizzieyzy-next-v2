import { useCallback, useEffect, useMemo, useState } from "react";
import type { AnalysisView, Backend, DocumentView, PointView, RestrictionDepth, RestrictionKind, SearchParams, Settings } from "@/api/backend";
import type { PanelCover } from "../frame/MainPanel";
import type { MenuBindings } from "../menu/bindings";
import type en from "../locales/en.json";
import type { AnalysisToolbar } from "./AnalysisToolbar";
import { ScoringPanel } from "./ScoringPanel";
import { SearchParamsDialog } from "./SearchParamsDialog";
import { DEFAULT_DEPTH, type ParamKey, paramsOrNull, toolsError } from "./tools";

type Labels = typeof en.analysisTools;

export type AnalysisToolsBackend = Pick<
  Backend,
  | "setAnalysisRestriction"
  | "addRestrictionPoints"
  | "removeRestrictionPoints"
  | "clearAnalysisRestriction"
  | "setRestrictionDepth"
  | "setSearchParams"
  | "continueLadder"
  | "startScoring"
  | "toggleScoringPoint"
  | "setScoringRule"
  | "commitScoring"
  | "cancelScoring"
>;

/** The bottom toolbar's 更新官方权重 and 更多 → 终局点目. */
export interface AnalysisBottomBar {
  onWeights(): void;
  scoring: boolean;
  canScore: boolean;
  onScore(): void;
}

export interface AnalysisToolsContext {
  labels: Labels;
  document: DocumentView | null;
  /** A game, an engine match or a coach game is on: Rust refuses these tools then. */
  busy: boolean;
  openRules(): void;
  openWeights(): void;
}

interface MenuItem {
  id: string;
  label: string;
  run(): void;
}

const same = (a: PointView, b: PointView) => a.x === b.x && a.y === b.y;

/** 选点限制, 引擎参数, 终局点目 and 自动补完征子: their entries, the board's marks and their windows. */
export function useAnalysisTools(backend: AnalysisToolsBackend, context: AnalysisToolsContext) {
  const { labels, document, busy } = context;
  const [selecting, setSelecting] = useState<RestrictionKind | null>(null);
  const [depths, setDepths] = useState({ allow: DEFAULT_DEPTH, avoid: DEFAULT_DEPTH });
  const [params, setParams] = useState<SearchParams | null>(null);
  const [paramsOpen, setParamsOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const restriction = document?.analysis_restriction ?? null;
  const scoring = document?.scoring ?? null;
  const usable = document?.board != null && !busy;
  // A game that starts, or 终局点目, puts the armed rectangle down.
  if (selecting !== null && (!usable || scoring)) setSelecting(null);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 3200);
    return () => window.clearTimeout(timer);
  }, [toast]);
  // Esc puts the armed rectangle down.
  useEffect(() => {
    if (selecting === null) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setSelecting(null);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selecting]);

  const load = useCallback((settings: Pick<Settings, "search_params" | "restriction_depths">) => {
    setParams(settings.search_params ?? null);
    setDepths({ allow: settings.restriction_depths?.allow ?? DEFAULT_DEPTH, avoid: settings.restriction_depths?.avoid ?? DEFAULT_DEPTH });
  }, []);
  const fail = (failure: unknown) => setToast(toolsError(failure, labels.errors));
  // The document event that follows shows the outcome; a refusal is said in a toast.
  const run = (request: Promise<unknown>) => void request.catch(fail);
  const clear = () => run(backend.clearAnalysisRestriction());
  const toggleScoring = () => run(scoring ? backend.cancelScoring() : backend.startScoring());
  const saveParams = (next: SearchParams | null, then?: () => void) =>
    void backend.setSearchParams(next).then((saved) => {
      setParams(saved);
      then?.();
    }, fail);

  const bindings: MenuBindings = {
    scoreGame: { run: toggleScoring, checked: scoring !== null, enabled: usable || scoring !== null },
    continueLadder: {
      run: () => void backend.continueLadder().then((done) => setToast(labels.ladder[done.outcome].replace("{n}", String(done.moves))), fail),
      enabled: usable && scoring === null,
    },
    engineParameters: { run: () => setParamsOpen(true) },
  };

  const toolbar: AnalysisToolbar = {
    enabled: usable && scoring === null,
    selecting,
    onSelect: (kind) => setSelecting(selecting === kind ? null : kind),
    depths,
    onDepth: (kind: RestrictionKind, depth: RestrictionDepth) =>
      void backend.setRestrictionDepth(kind, depth).then(() => setDepths((now) => ({ ...now, [kind === "Allow" ? "allow" : "avoid"]: depth })), fail),
    depthLabels: labels.depth,
    canClear: restriction !== null && !busy,
    onClear: clear,
    params,
    paramsEnabled: !busy,
    onParam: (key: ParamKey, value: number | null) => saveParams(paramsOrNull({ ...params, [key]: value })),
    onParams: () => setParamsOpen(true),
    onRules: context.openRules,
  };

  // Right click on an empty point, as Java's 只分析此点 / 增加 / 去除 / 不分析此点 / 清除.
  const menuItems = (point: PointView): MenuItem[] => {
    if (!usable || scoring) return [];
    const menu = labels.menu;
    const has = restriction?.points.some((p) => same(p, point)) ?? false;
    const items: MenuItem[] = [{ id: "allow-point", label: menu.allow, run: () => run(backend.setAnalysisRestriction("Allow", [point])) }];
    if (restriction?.kind === "Allow" && !has) items.push({ id: "add-allow", label: menu.addAllow, run: () => run(backend.addRestrictionPoints("Allow", [point])) });
    if (restriction?.kind === "Allow" && has) items.push({ id: "remove-allow", label: menu.removeAllow, run: () => run(backend.removeRestrictionPoints([point])) });
    if (!(restriction?.kind === "Avoid" && has)) items.push({ id: "avoid-point", label: menu.avoid, run: () => run(backend.addRestrictionPoints("Avoid", [point])) });
    if (restriction) items.push({ id: "clear-restriction", label: menu.clear, run: clear });
    return items;
  };

  const marks = useMemo(() => (restriction ? { kind: restriction.kind, points: restriction.points } : null), [restriction]);
  const board = {
    restriction: marks,
    scoring: scoring?.ownership ?? null,
    region: usable && !scoring ? { kind: selecting, onRegion: (kind: RestrictionKind, points: PointView[]) => {
      setSelecting(null);
      run(backend.addRestrictionPoints(kind, points));
    }, onClear: clear } : undefined,
    // While scoring, a click marks a group dead or a point neutral.
    ...(scoring ? { editing: true, onPoint: (point: PointView) => run(backend.toggleScoringPoint(point)) } : {}),
  };

  return {
    bindings,
    load,
    toolbar,
    bottom: { onWeights: context.openWeights, scoring: scoring !== null, canScore: usable || scoring !== null, onScore: toggleScoring } satisfies AnalysisBottomBar,
    board,
    menuItems,
    dialogOpen: paramsOpen,
    /** While restricted, the board and the list show Rust's restricted analysis of this position, never the plain one. */
    shown: (analysis: AnalysisView | null): AnalysisView | null => {
      if (!restriction) return analysis;
      const restricted = document?.restricted_analysis ?? null;
      return restricted && restricted.node_id === document?.tree.current ? restricted : null;
    },
    chip: restriction ? (
      <button type="button" className="chip restriction-chip" data-kind={restriction.kind} title={labels.chip.clearHint} disabled={busy} data-testid="restriction-chip" onClick={clear}>
        <span className="restriction-mark" aria-hidden="true" />
        <span className="chip-label">{labels.chip.title}</span>
        <strong>{labels.chip[restriction.kind].replace("{n}", String(restriction.points.length))}</strong>
      </button>
    ) : null,
    cover: scoring
      ? ({
          regions: ["list", "tree"],
          node: (
            <ScoringPanel view={scoring} labels={labels.scoring} onRule={(rule) => run(backend.setScoringRule(rule))} onConfirm={() => run(backend.commitScoring())} onCancel={() => run(backend.cancelScoring())} />
          ),
        } satisfies PanelCover)
      : undefined,
    windows: (
      <>
        {paramsOpen ? <SearchParamsDialog params={params} labels={labels.params} onSave={(next) => saveParams(next, () => setParamsOpen(false))} onClose={() => setParamsOpen(false)} /> : null}
        {toast ? (
          <div className="tools-toast" role="status" data-testid="tools-toast">
            {toast}
          </div>
        ) : null}
      </>
    ),
  };
}
