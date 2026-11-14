import { Fragment, type ReactNode, useRef, useState } from "react";
import type { GameToolbar } from "../play/usePlay";
import type { AnalysisLimitSettings } from "@/api/backend";
import { JAVA_ICONS } from "./icons";
import { ToolButton, ToolSeparator } from "./ToolButton";
import type { ItemFilter } from "./toolbarItems";

export interface TopToolbarLabels {
  newFile: string;
  open: string;
  save: string;
  fox: string;
  foxTip: string;
  tencent: string;
  tencentTip: string;
  strength: string;
  strengthTip: string;
  autoSetup: string;
  autoSetupTip: string;
  flash: string;
  search: string;
  hawkeye: string;
  rankMark: string;
  changeTurn: string;
  setMain: string;
  backMain: string;
  controlPanel: string;
  markup: string;
  playBlack: string;
  playWhite: string;
  playAlternately: string;
  playPass: string;
  selectAllow: string;
  allowOptions: string;
  selectAvoid: string;
  avoidOptions: string;
  clearSelect: string;
  newGame: string;
  pauseGame: string;
  komi: string;
  pda: string;
  wrn: string;
  candidates: string;
  black: string;
  white: string;
  rules: string;
  tsumego: string;
  params: string;
  goban: string;
  saveLoad: string;
  /** 单步分析限制: the time and visit fields Java offers in 自定义内容. */
  timeLimit: string;
  seconds: string;
  playoutLimit: string;
  /** 独立/合并工具栏按钮. */
  switchLayout: string;
}

export interface TopToolbarProps {
  labels: TopToolbarLabels;
  hasDocument: boolean;
  hasBoard: boolean;
  /** Whether 返回主分支 has somewhere to go. */
  offMainLine: boolean;
  komi: number | null;
  onNew(): void;
  onSetMain(): void;
  /** The markup tools are out. */
  markupOpen: boolean;
  onMarkup(): void;
  /** 超级鹰眼: the problem moves page is open. */
  hawkEye?: boolean;
  onHawkEye?(): void;
  /** 落子评价标记 is on; the button steps through Java's Alt+M cycle. */
  rankMarks?: boolean;
  onRankMarks?(): void;
  onChangeTurn(): void;
  /** 试下 is on: the buttons that would replace the game wait. */
  trial: boolean;
  /** 添加黑子 / 添加白子 in force, or null for alternate play. */
  playColor: "Black" | "White" | null;
  onColor(color: "Black" | "White"): void;
  onAlternate(): void;
  onKomi(komi: number): void;
  onBoardSize(): void;
  onOpen(): void;
  onSave(): void;
  onFox(): void;
  onStrength(): void;
  onAutoSetup(): void;
  onFlash(): void;
  onBackMain(): void;
  onPass(): void;
  onTsumego(): void;
  /** 显示 → 工具栏 → 自定义内容: which groups show. Every default group when absent. */
  show?: ItemFilter;
  /** 合并在菜单栏上: one row inside the menu bar, without the basic buttons. */
  combined?: boolean;
  /** 自动换行: more rows than Java's two when the window is narrow. */
  wrap?: boolean;
  /** 独立/合并工具栏按钮 switches between the two layouts. */
  onSwitchLayout?(): void;
  /** The per-move analysis limits, for the time and visit fields. */
  limits?: AnalysisLimitSettings | null;
  onLimits?(limits: AnalysisLimitSettings): void;
  /** 新对局 and, during a game, its controls. */
  game?: GameToolbar;
}

/** Java's default groups, for callers that do not customise the toolbar. */
const DEFAULT_SHOWN: ItemFilter = (id) => id !== "showDoubleMenuBtn" && id !== "showTimeLimitController" && id !== "showPlayoutLimitController";

/** A number field that saves on Enter or when left, like 贴目. */
function LimitField({ label, unit, value, enabled, checked, onCheck, onValue }: { label: string; unit?: string; value: number; enabled: boolean; checked: boolean; onCheck(checked: boolean): void; onValue(value: number): void }) {
  const [text, setText] = useState<string | null>(null);
  const commit = () => {
    const typed = text?.trim();
    setText(null);
    if (typed && /^\d+$/.test(typed) && Number(typed) > 0 && Number(typed) !== value) onValue(Number(typed));
  };
  return (
    <span className="jt-field jt-limit">
      <label className="jt-check">
        <input type="checkbox" checked={checked} disabled={!enabled} onChange={(event) => onCheck(event.target.checked)} />
        <span>{label}</span>
      </label>
      <input
        className="jt-input"
        inputMode="numeric"
        aria-label={label}
        disabled={!enabled}
        value={text ?? String(value)}
        onChange={(event) => setText(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
        }}
      />
      {unit ? <span>{unit}</span> : null}
    </span>
  );
}

function SearchGlyph() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="6.5" cy="6.5" r="4.6" fill="none" stroke="#1f2926" strokeWidth="1.8" />
      <path d="M10 10l4.2 4.2" stroke="#1f2926" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

/** 贴目: the game's komi, changed by typing a number and leaving the field or pressing Enter. */
function KomiField({ label, komi, onChange }: { label: string; komi: number | null; onChange(komi: number): void }) {
  // null while the field shows the game's own komi.
  const [text, setText] = useState<string | null>(null);
  const abandoned = useRef(false);
  const commit = () => {
    const typed = abandoned.current ? null : text;
    abandoned.current = false;
    setText(null);
    if (typed === null || typed.trim() === "") return;
    const value = Number(typed.trim());
    if (Number.isFinite(value) && value !== komi) onChange(value);
  };
  return (
    <label className="jt-field" aria-disabled={komi === null ? "true" : undefined}>
      <span>{label}</span>
      <span className="jt-spinner">
        <input
          className="jt-input"
          data-testid="tool-komi"
          inputMode="decimal"
          disabled={komi === null}
          value={text ?? (komi === null ? "" : String(komi))}
          onChange={(event) => setText(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Escape") abandoned.current = true;
            if (event.key === "Enter" || event.key === "Escape") event.currentTarget.blur();
          }}
        />
      </span>
    </label>
  );
}

/**
 * LizzieYzy Next's top toolbar in Java's order (`Menu` → `TopHeaderPanel`), wrapping onto a
 * second row when the window is narrower than its controls, as Java's does at 1429 px.
 */
export function TopToolbar({
  labels,
  hasDocument,
  hasBoard,
  offMainLine,
  komi,
  markupOpen,
  hawkEye = false,
  rankMarks = false,
  trial,
  playColor,
  show = DEFAULT_SHOWN,
  combined = false,
  wrap = false,
  limits = null,
  game,
  ...on
}: TopToolbarProps) {
  const icon = JAVA_ICONS;
  // Each group is shown or not; separators go only between groups that are.
  const groups: [string, ReactNode][] = [];
  const add = (id: string, node: ReactNode) => {
    if (show(id)) groups.push([id, node]);
  };
  const always = (id: string, node: ReactNode) => groups.push([id, node]);

  add("showDoubleMenuBtn", <ToolButton small label="⇵" title={labels.switchLayout} testId="tool-switch-layout" onClick={on.onSwitchLayout} enabled={on.onSwitchLayout !== undefined} />);
  if (!combined) {
    add(
      "showBasicBtn",
      <>
        <ToolButton icon={icon.newFile} title={labels.newFile} enabled={!trial} testId="tool-new" onClick={on.onNew} />
        <ToolButton icon={icon.open} title={labels.open} enabled={!trial} testId="open-file" onClick={on.onOpen} />
        <ToolButton icon={icon.save} title={labels.save} enabled={hasDocument} testId="save-file" onClick={on.onSave} />
        <ToolButton role="primary" label={labels.fox} title={labels.foxTip} enabled={!trial} testId="open-fox" onClick={on.onFox} />
        <ToolButton role="primary" label={labels.tencent} title={labels.tencentTip} enabled={false} />
        <ToolButton role="primary" label={labels.strength} title={labels.strengthTip} testId="open-strength" onClick={on.onStrength} />
        <ToolSeparator />
        <ToolButton role="primary" label={labels.autoSetup} title={labels.autoSetupTip} testId="tool-auto-setup" onClick={on.onAutoSetup} />
        <ToolButton icon={icon.flash} title={labels.flash} enabled={hasDocument} testId="tool-flash" onClick={on.onFlash} />
        <ToolButton
          className="jt-search"
          label={
            <>
              <SearchGlyph />
              {labels.search}
            </>
          }
          title={labels.search}
          enabled={false}
        />
      </>,
    );
  }
  always(
    "edit",
    <>
      <ToolButton icon={icon.hawkeye} title={labels.hawkeye} enabled={on.onHawkEye !== undefined && hasDocument} pressed={hawkEye} testId="tool-hawkeye" onClick={on.onHawkEye} />
      <ToolButton icon={icon.rankMark} title={labels.rankMark} enabled={on.onRankMarks !== undefined} pressed={rankMarks} testId="tool-rank-marks" onClick={on.onRankMarks} />
      <ToolButton icon={icon.changeTurn} title={labels.changeTurn} enabled={hasBoard} testId="tool-change-turn" onClick={on.onChangeTurn} />
      <ToolButton icon={icon.setMain} title={labels.setMain} enabled={offMainLine} testId="tool-set-main" onClick={on.onSetMain} />
      <ToolButton icon={icon.backMain} title={labels.backMain} enabled={offMainLine} testId="tool-back-main" onClick={on.onBackMain} />
      <ToolButton icon={icon.controlPanel} title={labels.controlPanel} enabled={false} />
      <ToolButton icon={icon.markup} title={labels.markup} enabled={hasBoard} pressed={markupOpen} testId="tool-markup" onClick={on.onMarkup} />
    </>,
  );
  add(
    "playMoveTools",
    <>
      <ToolButton icon={icon.black} title={labels.playBlack} enabled={hasBoard} pressed={playColor === "Black"} testId="tool-play-black" onClick={() => on.onColor("Black")} />
      <ToolButton icon={icon.white} title={labels.playWhite} enabled={hasBoard} pressed={playColor === "White"} testId="tool-play-white" onClick={() => on.onColor("White")} />
      <ToolButton icon={icon.alternately} title={labels.playAlternately} enabled={hasBoard} pressed={playColor === null} testId="tool-alternately" onClick={on.onAlternate} />
      <ToolButton icon={icon.pass} title={labels.playPass} enabled={hasBoard} testId="tool-pass" onClick={on.onPass} />
    </>,
  );
  add(
    "allowAvoidTools",
    <>
      <ToolButton icon={icon.allow} title={labels.selectAllow} enabled={false} />
      <ToolButton className="jt-narrow" icon={icon.more} title={labels.allowOptions} enabled={false} />
      <ToolButton icon={icon.avoid} title={labels.selectAvoid} enabled={false} />
      <ToolButton className="jt-narrow" icon={icon.more} title={labels.avoidOptions} enabled={false} />
      <ToolButton icon={icon.clear} title={labels.clearSelect} enabled={false} />
    </>,
  );
  add(
    "showGameController",
    <>
      <ToolButton role="primary" label={labels.newGame} title={labels.newGame} enabled={game?.canStart ?? false} testId="tool-new-game" onClick={game?.onNew} />
      {game?.active ? (
        <>
          {game.paused ? (
            <ToolButton label={game.labels.resume} title={game.labels.resume} testId="tool-resume-game" onClick={game.onResume} />
          ) : (
            <ToolButton label={labels.pauseGame} title={labels.pauseGame} testId="tool-pause-game" onClick={game.onPause} />
          )}
          <ToolButton label={game.labels.takeBack} title={game.labels.takeBack} testId="tool-take-back" onClick={game.onTakeBack} />
          <ToolButton role="danger" label={game.labels.resignGame} title={game.labels.resignGame} testId="tool-resign" onClick={game.onResign} />
          <ToolButton label={game.labels.endGame} title={game.labels.endGame} testId="tool-end-game" onClick={game.onEnd} />
        </>
      ) : (
        <ToolButton label={labels.pauseGame} title={labels.pauseGame} enabled={false} />
      )}
    </>,
  );
  const limitsReady = limits !== null && on.onLimits !== undefined;
  add(
    "showTimeLimitController",
    <span data-testid="tool-time-limit">
      <LimitField
        label={labels.timeLimit}
        unit={labels.seconds}
        value={limits?.max_analyze_time_seconds ?? 0}
        enabled={limitsReady}
        checked={limits?.limit_time ?? false}
        onCheck={(checked) => limits && on.onLimits?.({ ...limits, limit_time: checked })}
        onValue={(value) => limits && on.onLimits?.({ ...limits, limit_time: true, max_analyze_time_seconds: value })}
      />
    </span>,
  );
  add(
    "showPlayoutLimitController",
    <span data-testid="tool-playout-limit">
      <LimitField
        label={labels.playoutLimit}
        value={limits?.limit_playouts ?? 0}
        enabled={limitsReady}
        checked={limits?.limit_playout ?? false}
        onCheck={(checked) => limits && on.onLimits?.({ ...limits, limit_playout: checked })}
        onValue={(value) => limits && on.onLimits?.({ ...limits, limit_playout: true, limit_playouts: value })}
      />
    </span>,
  );
  always("komi", <KomiField label={labels.komi} komi={komi} onChange={on.onKomi} />);
  const pda = show("showPdaInToolbar");
  const wrn = show("showWrnInToolbar");
  if (pda || wrn) {
    always(
      "params-fields",
      <>
        {pda ? (
          <>
            <label className="jt-check" aria-disabled="true">
              <input type="checkbox" disabled />
              <span>{labels.pda}</span>
            </label>
            <input className="jt-input" value="0" disabled readOnly aria-label={labels.pda} data-testid="tool-pda" />
          </>
        ) : null}
        {wrn ? (
          <>
            <label className="jt-check" aria-disabled="true">
              <input type="checkbox" checked disabled readOnly />
              <span>{labels.wrn}</span>
            </label>
            <input className="jt-input" value="0.04" disabled readOnly aria-label={labels.wrn} data-testid="tool-wrn" />
          </>
        ) : null}
      </>,
    );
  }
  add(
    "showCandidateController",
    <>
      <span className="jt-field" aria-disabled="true">
        <span>{labels.candidates}</span>
      </span>
      <label className="jt-check">
        <input type="checkbox" checked disabled readOnly />
        <span>{labels.black}</span>
      </label>
      <label className="jt-check">
        <input type="checkbox" checked disabled readOnly />
        <span>{labels.white}</span>
      </label>
    </>,
  );
  const menus = (
    <>
      {show("showRuleMenu") ? <ToolButton small className="jt-rules" label={labels.rules} enabled={false} /> : null}
      {show("showTsumeGoMenu") ? <ToolButton small label={labels.tsumego} enabled={!trial} testId="tool-tsumego" onClick={on.onTsumego} /> : null}
      {show("showParamMenu") ? <ToolButton small label={labels.params} enabled={false} /> : null}
      {show("showGobanMenu") ? <ToolButton small label={labels.goban} enabled={!trial} testId="tool-goban" onClick={on.onBoardSize} /> : null}
      {show("showSaveLoadMenu") ? <ToolButton small label={labels.saveLoad} enabled={false} /> : null}
    </>
  );
  if (["showRuleMenu", "showTsumeGoMenu", "showParamMenu", "showGobanMenu", "showSaveLoadMenu"].some(show)) always("menus", menus);

  return (
    <div
      className={combined ? "jt-top jt-combined" : "jt-top"}
      role="toolbar"
      aria-label="LizzieYzy Next"
      data-testid="top-toolbar"
      data-layout={combined ? "Combined" : "Separate"}
      data-wrap={wrap ? "true" : "false"}
    >
      {groups.map(([id, node], index) => (
        <Fragment key={id}>
          {index > 0 ? <ToolSeparator /> : null}
          {node}
        </Fragment>
      ))}
    </div>
  );
}
