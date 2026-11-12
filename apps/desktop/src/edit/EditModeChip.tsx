export interface EditModeChipTool<T extends string> {
  value: T;
  label: string;
  /** A short glyph for the button; the label is its name. */
  glyph: string;
}

export interface EditModeChipProps<T extends string> {
  title: string;
  tools: readonly EditModeChipTool<T>[];
  tool: T | null;
  /** Extra commands after the tools: 黑先, 白先, 清空, 清除标记. */
  actions?: readonly { label: string; testId: string; enabled?: boolean; onClick(): void }[];
  done: string;
  doneHint: string;
  testId: string;
  onTool(tool: T): void;
  onDone(): void;
}

/**
 * The tool in hand while the board edits the starting position or the markup, in the strip's
 * tray beside the other status chips: the tools, a few commands and the way out (Esc too).
 */
export function EditModeChip<T extends string>({ title, tools, tool, actions = [], done, doneHint, testId, onTool, onDone }: EditModeChipProps<T>) {
  return (
    <span className="chip edit-chip" role="toolbar" aria-label={title} data-testid={testId}>
      <span className="edit-chip-mark" aria-hidden="true" />
      <span className="chip-label">{title}</span>
      <span className="edit-chip-tools">
        {tools.map((item) => (
          <button
            key={item.value}
            type="button"
            className="edit-chip-tool"
            aria-pressed={tool === item.value}
            aria-label={item.label}
            title={item.label}
            data-testid={`${testId}-${item.value.toLowerCase()}`}
            onClick={() => onTool(item.value)}
          >
            {item.glyph}
          </button>
        ))}
      </span>
      {actions.map((action) => (
        <button key={action.testId} type="button" className="edit-chip-action" data-testid={action.testId} disabled={action.enabled === false} onClick={action.onClick}>
          {action.label}
        </button>
      ))}
      <button type="button" className="tsumego-exit" data-testid={`${testId}-done`} title={doneHint} onClick={onDone}>
        {done}
      </button>
    </span>
  );
}
