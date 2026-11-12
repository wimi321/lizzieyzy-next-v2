import { useEffect, useState } from "react";
import { ToolButton } from "./ToolButton";

export interface ConfirmDialogProps {
  title: string;
  lines: readonly string[];
  /** "Do not ask again": Java puts the checkbox in the box itself. */
  noAgain: string;
  confirm: string;
  cancel: string;
  testId: string;
  onConfirm(noAgain: boolean): void;
  onCancel(): void;
}

/** Java's yes-or-no box: a question, "do not ask again", 确定 and 取消. Escape is 取消. */
export function ConfirmDialog({ title, lines, noAgain, confirm, cancel, testId, onConfirm, onCancel }: ConfirmDialogProps) {
  const [skip, setSkip] = useState(false);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onCancel]);
  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="limit-dialog confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby={`${testId}-title`} data-testid={testId}>
        <h2 id={`${testId}-title`}>{title}</h2>
        {lines.map((line) => (
          <p key={line}>{line}</p>
        ))}
        <label className="jt-check">
          <input type="checkbox" checked={skip} data-testid={`${testId}-no-again`} onChange={(event) => setSkip(event.target.checked)} />
          <span>{noAgain}</span>
        </label>
        <footer>
          <ToolButton role="primary" label={confirm} testId={`${testId}-ok`} onClick={() => onConfirm(skip)} />
          <ToolButton label={cancel} testId={`${testId}-cancel`} onClick={onCancel} />
        </footer>
      </section>
    </div>
  );
}
