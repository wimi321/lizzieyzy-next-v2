import { useEffect, useRef, useState } from "react";
import type { RestrictionDepth, RestrictionKind, SearchParams } from "@/api/backend";
import { JAVA_ICONS } from "../frame/icons";
import { ToolButton } from "../frame/ToolButton";
import type en from "../locales/en.json";
import { CUSTOM_DEPTH, type ParamKey, paramOf, paramRange } from "./tools";

type DepthLabels = typeof en.analysisTools.depth;

/** The top toolbar's 选点限制 group, 引擎参数 fields and 规则 / 参数 buttons. */
export interface AnalysisToolbar {
  /** A board and no game: the tools can be used. */
  enabled: boolean;
  /** The one-shot rectangle the toolbar armed, or null. */
  selecting: RestrictionKind | null;
  onSelect(kind: RestrictionKind): void;
  depths: { allow: RestrictionDepth; avoid: RestrictionDepth };
  onDepth(kind: RestrictionKind, depth: RestrictionDepth): void;
  depthLabels: DepthLabels;
  canClear: boolean;
  onClear(): void;
  params: SearchParams | null;
  /** Parameters can be changed now (not during a game). */
  paramsEnabled: boolean;
  onParam(key: ParamKey, value: number | null): void;
  onParams(): void;
  onRules(): void;
}

interface Titles {
  selectAllow: string;
  allowOptions: string;
  selectAvoid: string;
  avoidOptions: string;
  clearSelect: string;
}

/** 允许选点选项 / 排除选点选项: how deep into the search the restriction holds. */
function DepthButton({ title, kind, depth, labels, enabled, onDepth }: { title: string; kind: RestrictionKind; depth: RestrictionDepth; labels: DepthLabels; enabled: boolean; onDepth(depth: RestrictionDepth): void }) {
  const [anchor, setAnchor] = useState<{ left: number; top: number } | null>(null);
  const [custom, setCustom] = useState(String(depth.type === "Custom" ? depth.data : 5));
  const root = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!anchor) return;
    const close = (event: Event) => {
      if (event instanceof KeyboardEvent ? event.key === "Escape" : !(event.target instanceof Node) || !root.current?.contains(event.target)) setAnchor(null);
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", close);
    };
  }, [anchor]);

  const typed = /^\d+$/.test(custom.trim()) ? Number(custom.trim()) : null;
  const n = typed !== null && typed >= CUSTOM_DEPTH.min && typed <= CUSTOM_DEPTH.max ? typed : null;
  const choose = (next: RestrictionDepth) => {
    setAnchor(null);
    onDepth(next);
  };
  const id = kind.toLowerCase();
  return (
    <span ref={root} className="jt-depth">
      <ToolButton
        className="jt-narrow"
        icon={JAVA_ICONS.more}
        title={title}
        enabled={enabled}
        pressed={anchor !== null}
        testId={`tool-${id}-depth`}
        onClick={() => {
          const rect = root.current?.getBoundingClientRect();
          setAnchor(anchor || !rect ? null : { left: rect.left, top: rect.bottom + 2 });
          // The field opens on the saved depth, which may have been read since.
          setCustom(String(depth.type === "Custom" ? depth.data : 5));
        }}
      />
      {anchor ? (
        <div className="jt-more-menu depth-menu" role="menu" aria-label={title} style={anchor} data-testid={`${id}-depth-menu`}>
          <button type="button" role="menuitemradio" aria-checked={depth.type === "FirstMove"} data-testid="depth-first" onClick={() => choose({ type: "FirstMove" })}>
            {labels.firstMove}
          </button>
          <div className="depth-custom" role="menuitemradio" aria-checked={depth.type === "Custom"}>
            <button type="button" disabled={n === null} data-testid="depth-custom" onClick={() => n !== null && choose({ type: "Custom", data: n })}>
              {labels.custom}
            </button>
            <input
              className="jt-input"
              inputMode="numeric"
              value={custom}
              aria-label={labels.custom}
              aria-invalid={n === null}
              title={labels.customRange}
              data-testid="depth-custom-input"
              onChange={(event) => setCustom(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && n !== null) choose({ type: "Custom", data: n });
              }}
            />
          </div>
          <button type="button" role="menuitemradio" aria-checked={depth.type === "WholeSearch"} data-testid="depth-whole" onClick={() => choose({ type: "WholeSearch" })}>
            {labels.whole}
          </button>
        </div>
      ) : null}
    </span>
  );
}

/** 设置强制计算区域 ▼ 设置强制不计算区域 ▼ 清除强制区域, as Java lays them out. */
export function RestrictionButtons({ tools, titles, icons }: { tools: AnalysisToolbar | undefined; titles: Titles; icons: typeof JAVA_ICONS }) {
  const enabled = tools?.enabled ?? false;
  const select = (kind: RestrictionKind) => tools?.onSelect(kind);
  return (
    <>
      <ToolButton icon={icons.allow} title={titles.selectAllow} enabled={enabled} pressed={tools?.selecting === "Allow"} testId="tool-select-allow" onClick={() => select("Allow")} />
      {tools ? (
        <DepthButton title={titles.allowOptions} kind="Allow" depth={tools.depths.allow} labels={tools.depthLabels} enabled={enabled} onDepth={(depth) => tools.onDepth("Allow", depth)} />
      ) : (
        <ToolButton className="jt-narrow" icon={icons.more} title={titles.allowOptions} enabled={false} />
      )}
      <ToolButton icon={icons.avoid} title={titles.selectAvoid} enabled={enabled} pressed={tools?.selecting === "Avoid"} testId="tool-select-avoid" onClick={() => select("Avoid")} />
      {tools ? (
        <DepthButton title={titles.avoidOptions} kind="Avoid" depth={tools.depths.avoid} labels={tools.depthLabels} enabled={enabled} onDepth={(depth) => tools.onDepth("Avoid", depth)} />
      ) : (
        <ToolButton className="jt-narrow" icon={icons.more} title={titles.avoidOptions} enabled={false} />
      )}
      <ToolButton icon={icons.clear} title={titles.clearSelect} enabled={tools?.canClear ?? false} testId="tool-clear-restriction" onClick={tools?.onClear} />
    </>
  );
}

/** 激进度 / 广度: ticked when set; the number is saved on Enter or when the field is left. */
function ParamField({ label, paramKey, value, enabled, testId, onChange }: { label: string; paramKey: ParamKey; value: number | null; enabled: boolean; testId: string; onChange(value: number | null): void }) {
  const [text, setText] = useState<string | null>(null);
  const abandoned = useRef(false);
  const fallback = paramRange(paramKey).fallback;
  const commit = () => {
    const typed = abandoned.current ? null : text;
    abandoned.current = false;
    setText(null);
    if (typed === null) return;
    const parsed = paramOf(typed, paramKey);
    if (parsed !== undefined && parsed !== value) onChange(parsed);
  };
  return (
    <>
      <label className="jt-check">
        <input type="checkbox" checked={value !== null} disabled={!enabled} data-testid={`${testId}-on`} onChange={(event) => onChange(event.target.checked ? fallback : null)} />
        <span>{label}</span>
      </label>
      <input
        className="jt-input"
        inputMode="decimal"
        aria-label={label}
        disabled={!enabled}
        value={text ?? String(value ?? fallback)}
        data-testid={testId}
        onChange={(event) => setText(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Escape") abandoned.current = true;
          if (event.key === "Enter" || event.key === "Escape") event.currentTarget.blur();
        }}
      />
    </>
  );
}

/** The toolbar's PDA and WRN, the same values as 引擎参数; a change applies at once. */
export function ParamFields({ tools, pda, wrn, labels }: { tools: AnalysisToolbar | undefined; pda: boolean; wrn: boolean; labels: { pda: string; wrn: string } }) {
  const enabled = tools?.paramsEnabled ?? false;
  const value = (key: ParamKey) => tools?.params?.[key] ?? null;
  return (
    <>
      {pda ? (
        <ParamField label={labels.pda} paramKey="playout_doubling_advantage" value={value("playout_doubling_advantage")} enabled={enabled} testId="tool-pda" onChange={(next) => tools?.onParam("playout_doubling_advantage", next)} />
      ) : null}
      {wrn ? <ParamField label={labels.wrn} paramKey="wide_root_noise" value={value("wide_root_noise")} enabled={enabled} testId="tool-wrn" onChange={(next) => tools?.onParam("wide_root_noise", next)} /> : null}
    </>
  );
}
