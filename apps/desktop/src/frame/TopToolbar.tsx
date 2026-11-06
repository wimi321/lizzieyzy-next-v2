import { JAVA_ICONS } from "./icons";
import { ToolButton, ToolSeparator } from "./ToolButton";

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
}

export interface TopToolbarProps {
  labels: TopToolbarLabels;
  hasDocument: boolean;
  hasBoard: boolean;
  /** Whether 返回主分支 has somewhere to go. */
  offMainLine: boolean;
  komi: number | null;
  onOpen(): void;
  onSave(): void;
  onFox(): void;
  onStrength(): void;
  onAutoSetup(): void;
  onFlash(): void;
  onBackMain(): void;
  onPass(): void;
  onTsumego(): void;
}

function SearchGlyph() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="6.5" cy="6.5" r="4.6" fill="none" stroke="#1f2926" strokeWidth="1.8" />
      <path d="M10 10l4.2 4.2" stroke="#1f2926" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

/**
 * LizzieYzy Next's top toolbar in Java's order (`Menu` → `TopHeaderPanel`), wrapping onto a
 * second row when the window is narrower than its controls, as Java's does at 1429 px.
 */
export function TopToolbar({ labels, hasDocument, hasBoard, offMainLine, komi, ...on }: TopToolbarProps) {
  const icon = JAVA_ICONS;
  return (
    <div className="jt-top" role="toolbar" aria-label="LizzieYzy Next" data-testid="top-toolbar">
      <ToolButton icon={icon.newFile} title={labels.newFile} enabled={false} testId="tool-new" />
      <ToolButton icon={icon.open} title={labels.open} testId="open-file" onClick={on.onOpen} />
      <ToolButton icon={icon.save} title={labels.save} enabled={hasDocument} testId="save-file" onClick={on.onSave} />
      <ToolButton role="primary" label={labels.fox} title={labels.foxTip} testId="open-fox" onClick={on.onFox} />
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
      <ToolSeparator />
      <ToolButton icon={icon.hawkeye} title={labels.hawkeye} enabled={false} />
      <ToolButton icon={icon.rankMark} title={labels.rankMark} enabled={false} />
      <ToolButton icon={icon.changeTurn} title={labels.changeTurn} enabled={false} />
      <ToolButton icon={icon.setMain} title={labels.setMain} enabled={false} />
      <ToolButton icon={icon.backMain} title={labels.backMain} enabled={offMainLine} testId="tool-back-main" onClick={on.onBackMain} />
      <ToolButton icon={icon.controlPanel} title={labels.controlPanel} enabled={false} />
      <ToolButton icon={icon.markup} title={labels.markup} enabled={false} />
      <ToolSeparator />
      <ToolButton icon={icon.black} title={labels.playBlack} enabled={false} />
      <ToolButton icon={icon.white} title={labels.playWhite} enabled={false} />
      <ToolButton icon={icon.alternately} title={labels.playAlternately} enabled={false} />
      <ToolButton icon={icon.pass} title={labels.playPass} enabled={hasBoard} testId="tool-pass" onClick={on.onPass} />
      <ToolSeparator />
      <ToolButton icon={icon.allow} title={labels.selectAllow} enabled={false} />
      <ToolButton className="jt-narrow" icon={icon.more} title={labels.allowOptions} enabled={false} />
      <ToolButton icon={icon.avoid} title={labels.selectAvoid} enabled={false} />
      <ToolButton className="jt-narrow" icon={icon.more} title={labels.avoidOptions} enabled={false} />
      <ToolButton icon={icon.clear} title={labels.clearSelect} enabled={false} />
      <ToolSeparator />
      <ToolButton role="primary" label={labels.newGame} title={labels.newGame} enabled={false} />
      <ToolButton label={labels.pauseGame} title={labels.pauseGame} enabled={false} />
      <ToolSeparator />
      <label className="jt-field" aria-disabled="true">
        <span>{labels.komi}</span>
        <span className="jt-spinner">
          <input className="jt-input" data-testid="tool-komi" value={komi === null ? "" : String(komi)} disabled readOnly />
        </span>
      </label>
      <ToolSeparator />
      <label className="jt-check" aria-disabled="true">
        <input type="checkbox" disabled />
        <span>{labels.pda}</span>
      </label>
      <input className="jt-input" value="0" disabled readOnly aria-label={labels.pda} />
      <label className="jt-check" aria-disabled="true">
        <input type="checkbox" checked disabled readOnly />
        <span>{labels.wrn}</span>
      </label>
      <input className="jt-input" value="0.04" disabled readOnly aria-label={labels.wrn} />
      <ToolSeparator />
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
      <ToolSeparator />
      <ToolButton small className="jt-rules" label={labels.rules} enabled={false} />
      <ToolButton small label={labels.tsumego} testId="tool-tsumego" onClick={on.onTsumego} />
      <ToolButton small label={labels.params} enabled={false} />
      <ToolButton small label={labels.goban} enabled={false} />
      <ToolButton small label={labels.saveLoad} enabled={false} />
    </div>
  );
}
