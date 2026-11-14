import { type ReactNode, useEffect, useRef, useState } from "react";
import { ToolButton } from "./ToolButton";
import type { ItemFilter } from "./toolbarItems";

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

/** 更多: the buttons Java's bar has no room for at this width (终局点目, 纯网络). */
function MoreButton({ labels, trial, hasBoard, onTrial, show, pure, onPure }: { labels: BottomToolbarLabels; trial: boolean; hasBoard: boolean; onTrial(): void; show: ItemFilter; pure: boolean; onPure?(): void }) {
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
  return (
    <div ref={root}>
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
            <button type="button" role="menuitem" disabled>
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
  ...on
}: BottomToolbarProps) {
  const more = ["tryPlay", "finalScore", "heatMap"].some(show);
  return (
    <div className="jt-bottom" role="toolbar" data-testid="bottom-toolbar">
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
        <ToolButton role="primary" className="jt-optional" label={labels.remote} testId="tool-remote" onClick={on.onRemote} />
        <ToolButton className="jt-optional" label={labels.downloadWeight} enabled={false} />
        {show("kataEstimateButton") ? (
          <ToolButton label={labels.kataEstimate} pressed={ownership} enabled={hasBoard} testId="toggle-ownership" onClick={on.onToggleOwnership} />
        ) : null}
        {show("openfile") ? <ToolButton label={labels.open} enabled={on.onOpen !== undefined && !trial} testId="bottom-open" onClick={on.onOpen} /> : null}
        {show("savefile") ? <ToolButton label={labels.save} enabled={on.onSave !== undefined && hasBoard} testId="bottom-save" onClick={on.onSave} /> : null}
        {show("badMoves") ? <ToolButton label={labels.hawkEye} enabled={on.onHawkEye !== undefined && hasBoard} testId="bottom-hawkeye" onClick={on.onHawkEye} /> : null}
        {show("analyzeList") ? <ToolButton label={labels.list} enabled={on.onList !== undefined} testId="bottom-list" onClick={on.onList} /> : null}
        {show("refresh") ? <ToolButton label={labels.refresh} enabled={on.onRefresh !== undefined && hasBoard} testId="bottom-refresh" onClick={on.onRefresh} /> : null}
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
        {show("autoPlay") ? <ToolButton className="jt-optional" label={labels.autoPlay} enabled={false} /> : null}
      </div>
      <div className="jt-group" data-align="end">
        {show("setMainButton") ? <ToolButton className="jt-optional" label={labels.setMain} enabled={canSetMain} testId="bottom-set-main" onClick={on.onSetMain} /> : null}
        {show("backMain") ? <ToolButton label={labels.backMain} enabled={canBackMain} testId="bottom-back-main" onClick={on.onBackMain} /> : null}
        {show("moveRank") ? <ToolButton label={labels.moveRank} pressed={rankMarks} enabled={on.onMoveRank !== undefined} testId="bottom-move-rank" onClick={on.onMoveRank} /> : null}
        {show("move") ? (
          <ToolButton label={labels.move} pressed={moveNumbers} testId="toggle-move-numbers" enabled={on.onToggleMoveNumbers !== undefined} onClick={on.onToggleMoveNumbers} />
        ) : null}
        {show("coords") ? <ToolButton label={labels.coords} pressed={coordinates} testId="toggle-coordinates" onClick={on.onToggleCoordinates} /> : null}
        {show("clearButton") ? <ToolButton role="danger" label={labels.clear} enabled={!trial} testId="bottom-clear" onClick={on.onClear} /> : null}
        {show("deleteMove") ? <ToolButton role="danger" label={labels.delete} enabled={canDelete} testId="bottom-delete" onClick={on.onDelete} /> : null}
        {more ? <MoreButton labels={labels} trial={trial} hasBoard={hasBoard} onTrial={on.onTrial} show={show} pure={pureNetwork} onPure={on.onPureNetwork} /> : null}
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
