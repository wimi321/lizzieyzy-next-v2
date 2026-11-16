import { type ReactNode, useEffect, useRef, useState } from "react";
import { ToolButton, type ToolRole } from "./ToolButton";
import { useToolbarFit } from "./toolbarFit";
import type { ItemFilter } from "./toolbarItems";
import type { AutoPlayButton } from "../research/useResearch";
import type { AnalysisBottomBar } from "../analysis/useAnalysisTools";

/** Home / End step size: the backend stops at the first or last move. */
export const JUMP = 1_000_000;

export interface BottomToolbarLabels {
  analyse: string;
  pauseAnalyse: string;
  remote: string;
  downloadWeight: string;
  kataEstimate: string;
  gotoMove: string;
  autoPlay: string;
  setMain: string;
  move: string;
  coords: string;
  clear: string;
  delete: string;
  more: string;
  finalScore: string;
  heatMap: string;
  tryPlay: string;
  tryBack: string;
  open: string;
  save: string;
  hawkEye: string;
  list: string;
  refresh: string;
  backMain: string;
  moveRank: string;
  first: string;
  back10: string;
  previous: string;
  next: string;
  forward10: string;
  last: string;
}

export interface BottomToolbarProps {
  labels: BottomToolbarLabels;
  pondering: boolean;
  hasBoard: boolean;
  ownership: boolean;
  coordinates: boolean;
  /** 手数: move numbers are on the board. */
  moveNumbers?: boolean;
  /** Off the main line there is a branch to promote. */
  canSetMain: boolean;
  /** Anywhere but the root there is a move to delete. */
  canDelete: boolean;
  /** 试下 is on; 更多 offers 恢复 instead. */
  trial: boolean;
  /** The 自动分析 entry, which keeps its own panel. */
  autoAnalysis: ReactNode;
  /** 自动播放: pressed while the main board plays on by itself. */
  autoPlay?: AutoPlayButton;
  /** 更新官方权重 and 更多 → 终局点目. */
  tools?: AnalysisBottomBar;
  onTogglePondering(): void;
  onToggleOwnership(): void;
  onToggleCoordinates(): void;
  onToggleMoveNumbers?(): void;
  /** 跳转: the move number typed in the field. */
  onGotoMove?(moveNumber: number): void;
  onStep(steps: number): void;
  onSetMain(): void;
  onClear(): void;
  onDelete(): void;
  onTrial(): void;
  /** 远程算力 (P3-20). */
  onRemote(): void;
  /** 显示 → 工具栏 → 底部工具栏 → 自定义内容. Every default button when absent. */
  show?: ItemFilter;
  /** 纯网络 is on. */
  pureNetwork?: boolean;
  onPureNetwork?(): void;
  /** The buttons Java hides at first, shown once chosen. */
  onOpen?(): void;
  onSave?(): void;
  onHawkEye?(): void;
  onList?(): void;
  onRefresh?(): void;
  onBackMain?(): void;
  canBackMain?: boolean;
  onMoveRank?(): void;
  rankMarks?: boolean;
}

/** Java's default buttons, for callers that do not customise the bar. */
const DEFAULT_SHOWN: ItemFilter = (id) => !["openfile", "savefile", "badMoves", "analyzeList", "refresh", "backMain", "moveRank"].includes(id);

const NAVIGATION: { key: keyof BottomToolbarLabels; text: string; steps: number }[] = [
  { key: "first", text: "|<", steps: -JUMP },
  { key: "back10", text: "<<", steps: -10 },
  { key: "previous", text: "<", steps: -1 },
  { key: "next", text: ">", steps: 1 },
  { key: "forward10", text: ">>", steps: 10 },
  { key: "last", text: ">|", steps: JUMP },
];

/** A button that gives way when the window is narrow, as the bar shows it and as 更多 lists it. */
interface Optional {
  id: string;
  label: string;
  testId: string;
  enabled: boolean;
  pressed?: boolean;
  role?: ToolRole;
  className?: string;
  onClick?(): void;
}

/**
 * The order the optional buttons keep their place in as the window narrows: 远程算力 first, then
 * the everyday toggles; 自动播放 gives way first. What does not fit moves into 更多.
 */
const PRIORITY: readonly string[] = [
  "remote",
  "kataEstimateButton",
  "coords",
  "move",
  "deleteMove",
  "clearButton",
  "openfile",
  "savefile",
  "badMoves",
  "analyzeList",
  "refresh",
  "backMain",
  "moveRank",
  "downloadWeight",
  "setMainButton",
  "autoPlay",
];
const MORE = "more";

function MenuEntry({ item, onChosen }: { item: Optional; onChosen(): void }) {
  return (
    <button
      type="button"
      role="menuitem"
      data-on={item.pressed ? "true" : undefined}
      disabled={!item.enabled}
      data-testid={`more-${item.testId}`}
      onClick={() => {
        onChosen();
        item.onClick?.();
      }}
    >
      {item.label}
    </button>
  );
}

/** 更多: 试下, 终局点目 and 纯网络, and the bar's buttons that have no room at this width. */
function MoreButton({
  labels,
  trial,
  hasBoard,
  onTrial,
  show,
  pure,
  onPure,
  tools,
  overflow,
  hidden,
}: {
  labels: BottomToolbarLabels;
  trial: boolean;
  hasBoard: boolean;
  onTrial(): void;
  show: ItemFilter;
  pure: boolean;
  onPure?(): void;
  tools?: AnalysisBottomBar;
  overflow: readonly Optional[];
  hidden: boolean;
}) {
  const [anchor, setAnchor] = useState<{ right: number; bottom: number } | null>(null);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!anchor) return;
    const close = (event: PointerEvent) => {
      if (!(event.target instanceof Node) || !root.current?.contains(event.target)) setAnchor(null);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [anchor]);
  const own = ["tryPlay", "finalScore", "heatMap"].some(show);
  return (
    <div ref={root} data-fit={MORE} className={hidden ? "jt-fit-hidden" : undefined}>
      <ToolButton
        label={labels.more}
        testId="tool-more"
        onClick={() => {
          const rect = root.current?.getBoundingClientRect();
          setAnchor(anchor || !rect ? null : { right: window.innerWidth - rect.right, bottom: window.innerHeight - rect.top + 3 });
        }}
      />
      {anchor ? (
        <div className="jt-more-menu" role="menu" style={anchor} data-testid="tool-more-menu">
          {overflow.map((item) => (
            <MenuEntry key={item.id} item={item} onChosen={() => setAnchor(null)} />
          ))}
          {overflow.length > 0 && own ? <div className="jt-more-separator" role="separator" /> : null}
          {show("tryPlay") ? (
            <button
              type="button"
              role="menuitem"
              disabled={!hasBoard}
              data-testid="tool-try-play"
              onClick={() => {
                setAnchor(null);
                onTrial();
              }}
            >
              {trial ? labels.tryBack : labels.tryPlay}
            </button>
          ) : null}
          {show("finalScore") ? (
            <button
              type="button"
              role="menuitem"
              data-on={tools?.scoring ? "true" : undefined}
              disabled={!tools?.canScore}
              data-testid="tool-final-score"
              onClick={() => {
                setAnchor(null);
                tools?.onScore();
              }}
            >
              {labels.finalScore}
            </button>
          ) : null}
          {show("heatMap") ? (
            <button
              type="button"
              role="menuitem"
              data-on={pure ? "true" : undefined}
              disabled={!hasBoard || onPure === undefined}
              data-testid="tool-heat-map"
              onClick={() => {
                setAnchor(null);
                onPure?.();
              }}
            >
              {labels.heatMap}
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/** LizzieYzy Next's bottom toolbar with its default buttons (`BottomToolbar`, `Config`). */
export function BottomToolbar({
  labels,
  pondering,
  hasBoard,
  ownership,
  coordinates,
  moveNumbers = false,
  canSetMain,
  canDelete,
  trial,
  autoAnalysis,
  show = DEFAULT_SHOWN,
  pureNetwork = false,
  canBackMain = false,
  rankMarks = false,
  autoPlay,
  tools,
  ...on
}: BottomToolbarProps) {
  const own = ["tryPlay", "finalScore", "heatMap"].some(show);
  const when = (id: string, item: Omit<Optional, "id">): Optional[] => (show(id) ? [{ id, ...item }] : []);
  const left: Optional[] = [
    { id: "remote", role: "primary", label: labels.remote, enabled: true, testId: "tool-remote", onClick: on.onRemote },
    { id: "downloadWeight", label: labels.downloadWeight, enabled: tools !== undefined, testId: "tool-weights", onClick: tools?.onWeights },
    ...when("kataEstimateButton", { label: labels.kataEstimate, pressed: ownership, enabled: hasBoard, testId: "toggle-ownership", onClick: on.onToggleOwnership }),
    ...when("openfile", { label: labels.open, enabled: on.onOpen !== undefined && !trial, testId: "bottom-open", onClick: on.onOpen }),
    ...when("savefile", { label: labels.save, enabled: on.onSave !== undefined && hasBoard, testId: "bottom-save", onClick: on.onSave }),
    ...when("badMoves", { label: labels.hawkEye, enabled: on.onHawkEye !== undefined && hasBoard, testId: "bottom-hawkeye", onClick: on.onHawkEye }),
    ...when("analyzeList", { label: labels.list, enabled: on.onList !== undefined, testId: "bottom-list", onClick: on.onList }),
    ...when("refresh", { label: labels.refresh, enabled: on.onRefresh !== undefined && hasBoard, testId: "bottom-refresh", onClick: on.onRefresh }),
  ];
  const center: Optional[] = when("autoPlay", { label: labels.autoPlay, enabled: hasBoard && autoPlay !== undefined, pressed: autoPlay?.playing, testId: "bottom-auto-play", onClick: autoPlay?.onClick });
  const right: Optional[] = [
    ...when("setMainButton", { label: labels.setMain, enabled: canSetMain, testId: "bottom-set-main", onClick: on.onSetMain }),
    ...when("backMain", { label: labels.backMain, enabled: canBackMain, testId: "bottom-back-main", onClick: on.onBackMain }),
    ...when("moveRank", { label: labels.moveRank, pressed: rankMarks, enabled: on.onMoveRank !== undefined, testId: "bottom-move-rank", onClick: on.onMoveRank }),
    ...when("move", { label: labels.move, pressed: moveNumbers, enabled: on.onToggleMoveNumbers !== undefined, testId: "toggle-move-numbers", onClick: on.onToggleMoveNumbers }),
    ...when("coords", { label: labels.coords, pressed: coordinates, enabled: true, testId: "toggle-coordinates", onClick: on.onToggleCoordinates }),
    ...when("clearButton", { role: "danger", label: labels.clear, enabled: !trial, testId: "bottom-clear", onClick: on.onClear }),
    ...when("deleteMove", { role: "danger", label: labels.delete, enabled: canDelete, testId: "bottom-delete", onClick: on.onDelete }),
  ];

  const bar = useRef<HTMLDivElement>(null);
  const optional = [...left, ...center, ...right];
  // What the bar holds, so the fit is measured again when buttons come or go.
  const signature = [own, show("analyse"), show("batchOpen"), ...optional.map((item) => `${item.id}:${item.label}`)].join("|");
  const fit = useToolbarFit(bar, signature, PRIORITY, MORE, own);
  const hidden = new Set(fit?.hidden ?? (own ? [] : [MORE]));
  const button = (item: Optional) => (
    <ToolButton
      key={item.id}
      fit={item.id}
      role={item.role}
      className={hidden.has(item.id) ? "jt-fit-hidden" : undefined}
      label={item.label}
      pressed={item.pressed}
      enabled={item.enabled}
      testId={item.testId}
      onClick={item.onClick}
    />
  );
  return (
    <div
      ref={bar}
      className="jt-bottom"
      role="toolbar"
      data-testid="bottom-toolbar"
      data-fit-hidden={fit?.hidden.filter((id) => id !== MORE).join(" ") || undefined}
      style={fit && !fit.centered ? { gridTemplateColumns: `${fit.leftTrack}px auto minmax(0, 1fr)` } : undefined}
    >
      <div className="jt-group">
        {show("analyse") ? (
          <ToolButton
            role="primary"
            label={
              <>
                <span className={pondering ? "jt-pause-glyph" : "jt-play-glyph"} aria-hidden="true" />
                {pondering ? labels.pauseAnalyse : labels.analyse}
              </>
            }
            testId="toggle-pondering"
            pressed={!pondering}
            onClick={on.onTogglePondering}
          />
        ) : null}
        {show("batchOpen") ? autoAnalysis : null}
        {left.map(button)}
      </div>
      <div className="jt-group">
        {NAVIGATION.map((item) => (
          <ToolButton
            key={item.key}
            className="jt-nav"
            label={item.text}
            title={labels[item.key]}
            enabled={hasBoard}
            testId={`nav-${item.key}`}
            onClick={() => on.onStep(item.steps)}
          />
        ))}
        <GotoMove label={labels.gotoMove} enabled={hasBoard && on.onGotoMove !== undefined} onGo={(n) => on.onGotoMove?.(n)} />
        {center.map(button)}
      </div>
      <div className="jt-group" data-align="end">
        {right.map(button)}
        <MoreButton
          labels={labels}
          trial={trial}
          hasBoard={hasBoard}
          onTrial={on.onTrial}
          show={show}
          pure={pureNetwork}
          onPure={on.onPureNetwork}
          tools={tools}
          overflow={optional.filter((item) => hidden.has(item.id))}
          hidden={hidden.has(MORE)}
        />
      </div>
    </div>
  );
}

/** Java's move-number field and 跳转: Enter or the button goes there and empties the field. */
function GotoMove({ label, enabled, onGo }: { label: string; enabled: boolean; onGo(moveNumber: number): void }) {
  const [text, setText] = useState("");
  const go = () => {
    const trimmed = text.trim();
    setText("");
    if (/^\d+$/.test(trimmed)) onGo(Number(trimmed));
  };
  return (
    <>
      <input
        className="jt-input"
        aria-label={label}
        inputMode="numeric"
        disabled={!enabled}
        value={text}
        data-testid="goto-move"
        onChange={(event) => setText(event.target.value.replace(/\D/g, ""))}
        onKeyDown={(event) => {
          if (event.key === "Enter") go();
        }}
      />
      <ToolButton label={label} enabled={enabled && text.trim() !== ""} testId="goto-move-button" onClick={go} />
    </>
  );
}
