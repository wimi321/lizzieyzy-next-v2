import { type ReactNode, useEffect, useRef, useState } from "react";

export type MenuItem =
  | { kind: "action"; label: string; testId: string; onSelect(): void; disabled?: boolean }
  | { kind: "check"; label: string; testId: string; checked: boolean; onSelect(): void }
  | { kind: "separator"; key: string };

export interface MenuDefinition {
  label: string;
  testId: string;
  items: MenuItem[];
}

/**
 * The in-window menu bar of LizzieYzy Next: menus open on click, follow the
 * pointer while one is open, and close on Escape or a click elsewhere.
 * `children` sit at the right end, where the Java app shows the engine.
 */
export function MenuBar({ menus, children }: { menus: MenuDefinition[]; children?: ReactNode }) {
  const [open, setOpen] = useState<number | null>(null);
  const bar = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open === null) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!(event.target instanceof Node) || !bar.current?.contains(event.target)) setOpen(null);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(null);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div className="menubar" ref={bar}>
      <div role="menubar" className="menubar-menus">
        {menus.map((menu, index) => (
          <div key={menu.testId} className="menu">
            <button
              type="button"
              role="menuitem"
              aria-haspopup="menu"
              aria-expanded={open === index}
              className="menu-title"
              data-testid={menu.testId}
              onClick={() => setOpen(open === index ? null : index)}
              onMouseEnter={() => open !== null && open !== index && setOpen(index)}
            >
              {menu.label}
            </button>
            {open === index ? (
              <div role="menu" className="menu-dropdown" aria-label={menu.label}>
                {menu.items.map((item) =>
                  item.kind === "separator" ? (
                    <div key={item.key} role="separator" className="menu-separator" />
                  ) : (
                    <button
                      key={item.testId}
                      type="button"
                      role={item.kind === "check" ? "menuitemcheckbox" : "menuitem"}
                      aria-checked={item.kind === "check" ? item.checked : undefined}
                      className="menu-item"
                      data-testid={item.testId}
                      disabled={item.kind === "action" && item.disabled}
                      onClick={() => {
                        setOpen(null);
                        item.onSelect();
                      }}
                    >
                      <span className="menu-check" aria-hidden="true">
                        {item.kind === "check" && item.checked ? "✓" : ""}
                      </span>
                      {item.label}
                    </button>
                  ),
                )}
              </div>
            ) : null}
          </div>
        ))}
      </div>
      <div className="menubar-status">{children}</div>
    </div>
  );
}
