import { type FormEvent, useEffect, useState } from "react";

export interface BoardSizeLabels {
  boardSizeTitle: string;
  other: string;
  sizeInvalid: string;
  confirm: string;
  cancel: string;
  close: string;
}

export interface BoardSizeDialogProps {
  width: number;
  height: number;
  labels: BoardSizeLabels;
  onConfirm(width: number, height: number): void;
  onClose(): void;
}

const PRESETS = [19, 15, 13, 9] as const;
/** The sizes the rules engine plays on. */
const MIN = 2;
const MAX = 25;

function side(text: string): number | null {
  if (!/^\d+$/.test(text.trim())) return null;
  const value = Number(text.trim());
  return value >= MIN && value <= MAX ? value : null;
}

/** 编辑 → 设置棋盘大小 (`SetBoardSize`): 19, 15, 13, 9 or any width by height. */
export function BoardSizeDialog({ width, height, labels, onConfirm, onClose }: BoardSizeDialogProps) {
  const preset = width === height && PRESETS.some((size) => size === width) ? width : null;
  const [choice, setChoice] = useState<number | "other">(preset ?? "other");
  const [columns, setColumns] = useState(String(width));
  const [rows, setRows] = useState(String(height));

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const custom = choice === "other";
  const chosen = custom ? { width: side(columns), height: side(rows) } : { width: choice, height: choice };
  const valid = chosen.width !== null && chosen.height !== null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (chosen.width !== null && chosen.height !== null) onConfirm(chosen.width, chosen.height);
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog edit-dialog" role="dialog" aria-modal="true" aria-labelledby="board-size-title" data-testid="board-size-dialog">
        <header className="dialog-header">
          <h2 id="board-size-title">{labels.boardSizeTitle}</h2>
          <button type="button" className="dialog-close" aria-label={labels.close} onClick={onClose}>
            ×
          </button>
        </header>
        <form className="edit-form" noValidate onSubmit={submit}>
          <div className="segmented board-size-choice" role="radiogroup" aria-labelledby="board-size-title">
            {PRESETS.map((size) => (
              <button key={size} type="button" role="radio" aria-checked={choice === size} data-testid={`board-size-${size}`} onClick={() => setChoice(size)}>
                {`${size}x${size}`}
              </button>
            ))}
            <button type="button" role="radio" aria-checked={custom} data-testid="board-size-other" onClick={() => setChoice("other")}>
              {labels.other}
            </button>
          </div>
          <div className="board-size-custom" data-active={custom}>
            <input
              className="text-input"
              inputMode="numeric"
              aria-label={`${labels.other} x`}
              aria-invalid={custom && chosen.width === null}
              data-testid="board-size-width"
              disabled={!custom}
              value={columns}
              onChange={(event) => setColumns(event.target.value)}
            />
            <span aria-hidden="true">x</span>
            <input
              className="text-input"
              inputMode="numeric"
              aria-label={`${labels.other} y`}
              aria-invalid={custom && chosen.height === null}
              data-testid="board-size-height"
              disabled={!custom}
              value={rows}
              onChange={(event) => setRows(event.target.value)}
            />
            {custom && !valid ? <span className="edit-invalid">{labels.sizeInvalid}</span> : null}
          </div>
          <footer className="dialog-footer">
            <button type="button" className="secondary-button" onClick={onClose}>
              {labels.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="board-size-confirm" disabled={!valid}>
              {labels.confirm}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
