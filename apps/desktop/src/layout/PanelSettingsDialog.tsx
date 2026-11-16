import { useEffect, useRef } from "react";
import { type Display, PANEL_FIELDS } from "../display/settings";
import type { FloatingLayout } from "../floating/useFloating";
import type en from "../locales/en.json";

type Labels = typeof en.layout.dialog;

export interface PanelSettingsDialogProps {
  /** null for 可视化面板设置, else the custom layout slot being set (0 or 1). */
  slot: number | null;
  display: Display;
  labels: Labels;
  /** A change shows at once in the main window. */
  onChange(patch: Partial<Display>): void;
  onApply(): void;
  onCancel(): void;
  /** 浮动主棋盘 and 浮动小棋盘, which open and close their windows at once. */
  floating?: { layout: FloatingLayout; onChange(next: FloatingLayout): void };
}

/**
 * 可视化面板设置 and 设置自定义布局 N (Java's `SetCustomMode`): every change shows at once; 应用
 * keeps it, 取消 or Esc puts the window back as it was when the dialog opened.
 */
export function PanelSettingsDialog({ slot, display, labels, onChange, onApply, onCancel, floating }: PanelSettingsDialogProps) {
  const latest = useRef(onCancel);
  useEffect(() => {
    latest.current = onCancel;
  });
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") latest.current();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const enlarged = display.large_sub_board || display.large_winrate_graph;
  // Java greys the enlargements while the board floats, and the sub-board switch while it does.
  const mainFloats = floating ? floating.layout.main && !floating.layout.keep : false;
  const subFloats = floating?.layout.sub ?? false;
  const check = (field: keyof Display, label: string, disabled = false) => (
    <label key={field} className="jt-check panel-settings-check">
      <input
        type="checkbox"
        checked={display[field] === true}
        disabled={disabled}
        data-testid={`panel-${field}`}
        onChange={(event) => onChange({ [field]: event.target.checked })}
      />
      <span>{label}</span>
    </label>
  );
  const title = slot === null ? labels.title : labels.customTitle.replace("{n}", String(slot + 1));
  return (
    <div className="dialog-backdrop panel-settings-backdrop" role="presentation">
      <section className="dialog panel-settings" role="dialog" aria-modal="true" aria-labelledby="panel-settings-title" data-testid="panel-settings">
        <header className="dialog-header">
          <h2 id="panel-settings-title">{title}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onCancel}>
            ×
          </button>
        </header>
        <div className="panel-settings-grid">{PANEL_FIELDS.map((field) => check(field, labels.panels[field], field === "show_sub_board" && subFloats))}</div>
        <div className="panel-settings-grid">
          {check("large_sub_board", labels.largeSubBoard, mainFloats)}
          {check("large_winrate_graph", labels.largeWinrateGraph, mainFloats)}
          {floating ? (
            <>
              <label className="jt-check panel-settings-check">
                <input type="checkbox" checked={mainFloats} data-testid="panel-float-main" onChange={(event) => floating.onChange({ ...floating.layout, main: event.target.checked, keep: false })} />
                <span>{labels.floatMain}</span>
              </label>
              <label className="jt-check panel-settings-check">
                <input type="checkbox" checked={subFloats} data-testid="panel-float-sub" onChange={(event) => floating.onChange({ ...floating.layout, sub: event.target.checked })} />
                <span>{labels.floatSub}</span>
              </label>
            </>
          ) : null}
        </div>
        <label className="panel-settings-position">
          <span>{labels.position}</span>
          <input
            type="range"
            min={0}
            max={8}
            step={1}
            value={display.board_position}
            disabled={enlarged && !mainFloats}
            data-testid="panel-position"
            onChange={(event) => onChange({ board_position: Number(event.target.value) })}
          />
          <output>{display.board_position}</output>
        </label>
        <footer className="dialog-footer">
          <button type="button" className="secondary-button" onClick={onCancel}>
            {labels.cancel}
          </button>
          <button type="button" className="primary-button" data-testid="panel-settings-apply" onClick={onApply}>
            {labels.apply}
          </button>
        </footer>
      </section>
    </div>
  );
}
