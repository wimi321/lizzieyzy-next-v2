import { type ReactNode, useEffect, useRef, useState } from "react";
import { ToolButton } from "./ToolButton";

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
}

const NAVIGATION: { key: keyof BottomToolbarLabels; text: string; steps: number }[] = [
  { key: "first", text: "|<", steps: -JUMP },
  { key: "back10", text: "<<", steps: -10 },
  { key: "previous", text: "<", steps: -1 },
  { key: "next", text: ">", steps: 1 },
  { key: "forward10", text: ">>", steps: 10 },
  { key: "last", text: ">|", steps: JUMP },
];

/** 更多: the buttons Java's bar has no room for at this width (终局点目, 纯网络). */
function MoreButton({ labels, trial, hasBoard, onTrial }: { labels: BottomToolbarLabels; trial: boolean; hasBoard: boolean; onTrial(): void }) {
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
          <button type="button" role="menuitem" disabled>
            {labels.finalScore}
          </button>
          <button type="button" role="menuitem" disabled>
            {labels.heatMap}
          </button>
        </div>
      ) : null}
    </div>
  );
}

/** LizzieYzy Next's bottom toolbar with its default buttons (`BottomToolbar`, `Config`). */
export function BottomToolbar({ labels, pondering, hasBoard, ownership, coordinates, moveNumbers = false, canSetMain, canDelete, trial, autoAnalysis, ...on }: BottomToolbarProps) {
  return (
    <div className="jt-bottom" role="toolbar" data-testid="bottom-toolbar">
      <div className="jt-group">
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
        {autoAnalysis}
        <ToolButton role="primary" className="jt-optional" label={labels.remote} enabled={false} />
        <ToolButton className="jt-optional" label={labels.downloadWeight} enabled={false} />
        <ToolButton label={labels.kataEstimate} pressed={ownership} enabled={hasBoard} testId="toggle-ownership" onClick={on.onToggleOwnership} />
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
        <ToolButton className="jt-optional" label={labels.autoPlay} enabled={false} />
      </div>
      <div className="jt-group" data-align="end">
        <ToolButton className="jt-optional" label={labels.setMain} enabled={canSetMain} testId="bottom-set-main" onClick={on.onSetMain} />
        <ToolButton label={labels.move} pressed={moveNumbers} testId="toggle-move-numbers" enabled={on.onToggleMoveNumbers !== undefined} onClick={on.onToggleMoveNumbers} />
        <ToolButton label={labels.coords} pressed={coordinates} testId="toggle-coordinates" onClick={on.onToggleCoordinates} />
        <ToolButton role="danger" label={labels.clear} enabled={!trial} testId="bottom-clear" onClick={on.onClear} />
        <ToolButton role="danger" label={labels.delete} enabled={canDelete} testId="bottom-delete" onClick={on.onDelete} />
        <MoreButton labels={labels} trial={trial} hasBoard={hasBoard} onTrial={on.onTrial} />
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
