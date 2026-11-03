/** The engine status at the bottom of the left column, as Java's `StatusOverlay` draws it. */
export function StatusLine({ text }: { text: string }) {
  return text ? (
    <p className="status-line" data-testid="status-line" title={text}>
      {text}
    </p>
  ) : null;
}
