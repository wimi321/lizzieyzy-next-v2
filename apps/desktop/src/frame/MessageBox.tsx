import { useEffect } from "react";
import { ToolButton } from "./ToolButton";

export interface MessageBoxProps {
  title: string;
  lines: readonly string[];
  ok: string;
  testId: string;
  onClose(): void;
}

/** A small message box in the style of Java's: it says what happened and goes away on OK or Escape. */
export function MessageBox({ title, lines, ok, testId, onClose }: MessageBoxProps) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);
  return (
    <section className="limit-dialog message-box" role="alertdialog" aria-modal="false" aria-labelledby={`${testId}-title`} data-testid={testId}>
      <h2 id={`${testId}-title`}>{title}</h2>
      {lines.map((line) => (
        <p key={line}>{line}</p>
      ))}
      <footer>
        <ToolButton label={ok} testId={`${testId}-ok`} onClick={onClose} />
      </footer>
    </section>
  );
}
