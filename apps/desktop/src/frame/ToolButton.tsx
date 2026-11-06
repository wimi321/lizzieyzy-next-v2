import type { ReactNode } from "react";

/** Java's button roles: plain white, the accent (green fill, blue border) and danger (pink). */
export type ToolRole = "normal" | "primary" | "danger";

export interface ToolButtonProps {
  /** Text on the button; icon-only buttons name themselves through `title`. */
  label?: ReactNode;
  icon?: string;
  title?: string;
  role?: ToolRole;
  /** The short buttons at the end of the top toolbar (规则, 死活, …). */
  small?: boolean;
  /** Features the new app does not have yet stay in place, greyed, as Java greys its own. */
  enabled?: boolean;
  pressed?: boolean;
  className?: string;
  testId?: string;
  onClick?(): void;
}

/** One of LizzieYzy Next's toolbar buttons. */
export function ToolButton({ label, icon, title, role = "normal", small, enabled = true, pressed, className, testId, onClick }: ToolButtonProps) {
  return (
    <button
      type="button"
      className={className ? `jt-button ${className}` : "jt-button"}
      data-role={role === "normal" ? undefined : role}
      data-icon={icon && label === undefined ? "" : undefined}
      data-small={small ? "" : undefined}
      aria-pressed={pressed}
      aria-label={label === undefined ? title : undefined}
      title={title}
      data-testid={testId}
      disabled={!enabled}
      onClick={onClick}
    >
      {icon ? <img src={icon} alt="" draggable={false} /> : null}
      {label}
    </button>
  );
}

export function ToolSeparator() {
  return <span className="jt-separator" aria-hidden="true" />;
}
