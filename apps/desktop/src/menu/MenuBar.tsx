import { type KeyboardEvent, type ReactNode, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { isEnabled, type MenuBindings } from "./bindings";
import { splitHint } from "./hint";
import { labelOf, type MenuEntry, type MenuSubmenu } from "./spec";

export interface MenuBarProps {
  menus: readonly MenuSubmenu[];
  bindings: MenuBindings;
  /** Labels by legacy key. */
  labels: Readonly<Record<string, string>>;
  label: string;
  /** Status chips at the strip's right end, such as board synchronization. */
  tray?: ReactNode;
}

/** One open popup: the top menu's at depth 0, then each open submenu. */
interface Level {
  entries: readonly MenuEntry[];
  x: number;
  y: number;
  /** A submenu flips to this edge when it does not fit on the right. */
  flipX?: number;
  /** The parent row a submenu belongs to. */
  from?: number;
  active: number | null;
  placed: boolean;
}

/** Java opens a submenu after a short hover; moving away closes it after the same delay. */
const HOVER_DELAY = 150;
const EDGE = 4;

function opensSubmenu(entry: MenuEntry | undefined): entry is MenuSubmenu {
  return entry?.kind === "submenu" && entry.items.length > 0;
}

function selectable(entry: MenuEntry | undefined, bindings: MenuBindings): boolean {
  if (!entry || entry.kind === "separator") return false;
  return entry.kind === "submenu" ? opensSubmenu(entry) : isEnabled(bindings[entry.id]);
}

/** The next selectable row from `from` in `direction`, wrapping; `from` null starts at an end. */
function nextSelectable(entries: readonly MenuEntry[], bindings: MenuBindings, from: number | null, direction: 1 | -1): number | null {
  const count = entries.length;
  let index = from ?? (direction === 1 ? -1 : count);
  for (let i = 0; i < count; i += 1) {
    index = (index + direction + count) % count;
    if (selectable(entries[index], bindings)) return index;
  }
  return null;
}

/**
 * LizzieYzy Next's menu strip, drawn in the window as Java draws it on Windows and macOS.
 * Popups are fixed-position siblings in one layer, so long menus can scroll without
 * clipping their submenus.
 */
export function MenuBar({ menus, bindings, labels, label, tray }: MenuBarProps) {
  const uid = useId();
  const [top, setTop] = useState<number | null>(null);
  const [focusTop, setFocusTop] = useState(0);
  const [levels, setLevels] = useState<Level[]>([]);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const popups = useRef<(HTMLDivElement | null)[]>([]);
  const rows = useRef(new Map<string, HTMLDivElement>());
  const strip = useRef<HTMLDivElement>(null);
  const layer = useRef<HTMLDivElement>(null);
  const timer = useRef<number | undefined>(undefined);
  // Hover timers fire after a render; they read the levels as they are then.
  const latest = useRef(levels);
  useEffect(() => {
    latest.current = levels;
  });

  const clearTimer = () => window.clearTimeout(timer.current);

  const close = (refocus: boolean) => {
    clearTimer();
    const open = top;
    setTop(null);
    setLevels([]);
    if (refocus && open !== null) buttons.current[open]?.focus();
    else if (strip.current?.contains(document.activeElement)) (document.activeElement as HTMLElement).blur();
  };

  const openTop = (index: number, highlightFirst: boolean) => {
    const menu = menus[index];
    const rect = buttons.current[index]?.getBoundingClientRect();
    if (!menu || !rect) return;
    clearTimer();
    setTop(index);
    setFocusTop(index);
    setLevels([
      {
        entries: menu.items,
        x: rect.left,
        y: rect.bottom + 2,
        active: highlightFirst ? nextSelectable(menu.items, bindings, null, 1) : null,
        placed: false,
      },
    ]);
  };

  const openChild = (depth: number, index: number, highlightFirst: boolean) => {
    const entry = levels[depth]?.entries[index];
    const rect = rows.current.get(`${depth}:${index}`)?.getBoundingClientRect();
    if (!opensSubmenu(entry) || !rect) return;
    const child: Level = {
      entries: entry.items,
      x: rect.right + 2,
      y: rect.top - 6,
      flipX: rect.left - 2,
      from: index,
      active: highlightFirst ? nextSelectable(entry.items, bindings, null, 1) : null,
      placed: false,
    };
    setLevels((current) => {
      const parent = current[depth];
      return parent ? [...current.slice(0, depth), { ...parent, active: index }, child] : current;
    });
  };

  const highlight = (depth: number, index: number | null) => {
    setLevels((current) =>
      current[depth]?.active === index ? current : current.map((level, i) => (i === depth ? { ...level, active: index } : level)),
    );
  };

  const activate = (depth: number, index: number) => {
    const entry = levels[depth]?.entries[index];
    if (!entry || entry.kind === "separator") return;
    if (entry.kind === "submenu") {
      clearTimer();
      openChild(depth, index, true);
      return;
    }
    const binding = bindings[entry.id];
    if (!isEnabled(binding)) return;
    close(false);
    binding?.run?.();
  };

  const hoverRow = (depth: number, index: number) => {
    const entry = levels[depth]?.entries[index];
    clearTimer();
    highlight(depth, selectable(entry, bindings) ? index : null);
    timer.current = window.setTimeout(() => {
      const child = latest.current[depth + 1];
      if (!opensSubmenu(entry)) setLevels((current) => current.slice(0, depth + 1));
      else if (child?.from !== index) openChild(depth, index, false);
    }, HOVER_DELAY);
  };

  // Keyboard focus follows the deepest open popup.
  const depth = levels.length - 1;
  useEffect(() => {
    if (depth >= 0) popups.current[depth]?.focus({ preventScroll: true });
  }, [depth, top]);

  // Keep every popup on screen: flip submenus left, pull long menus up.
  useLayoutEffect(() => {
    const fixes = levels.map((level, i) => {
      const element = popups.current[i];
      if (level.placed || !element) return null;
      const { width, height } = element.getBoundingClientRect();
      let x = level.x;
      if (x + width > window.innerWidth - EDGE) {
        x = level.flipX !== undefined && level.flipX - width >= EDGE ? level.flipX - width : window.innerWidth - width - EDGE;
      }
      const y = Math.max(EDGE, Math.min(level.y, window.innerHeight - height - EDGE));
      return { x: Math.max(EDGE, x), y };
    });
    if (fixes.some((fix) => fix !== null)) {
      setLevels((current) => current.map((level, i) => (fixes[i] ? { ...level, ...fixes[i], placed: true } : level)));
    }
  }, [levels]);

  // Clicking elsewhere, leaving the window or resizing it closes the menus.
  useEffect(() => {
    if (top === null) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!strip.current?.contains(target) && !layer.current?.contains(target)) close(false);
    };
    const onLeave = () => close(false);
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("blur", onLeave);
    window.addEventListener("resize", onLeave);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("blur", onLeave);
      window.removeEventListener("resize", onLeave);
    };
  });

  // F10 reaches the menu bar from anywhere, as in Java's Windows look.
  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "F10" || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
      event.preventDefault();
      setFocusTop(0);
      buttons.current[0]?.focus();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => clearTimer, []);

  const moveTop = (from: number, direction: 1 | -1, open: boolean) => {
    const next = (from + direction + menus.length) % menus.length;
    if (open) openTop(next, true);
    else {
      setFocusTop(next);
      buttons.current[next]?.focus();
    }
  };

  const onButtonKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      openTop(index, true);
    } else if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
      event.preventDefault();
      moveTop(index, event.key === "ArrowRight" ? 1 : -1, top !== null);
    } else if (event.key === "Escape") {
      close(false);
    }
  };

  const onPopupKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const level = levels[depth];
    if (!level || top === null) return;
    const handled = () => {
      event.preventDefault();
      event.stopPropagation();
    };
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp":
        handled();
        highlight(depth, nextSelectable(level.entries, bindings, level.active, event.key === "ArrowDown" ? 1 : -1));
        break;
      case "Home":
      case "End":
        handled();
        highlight(depth, nextSelectable(level.entries, bindings, null, event.key === "Home" ? 1 : -1));
        break;
      case "ArrowRight":
        handled();
        if (level.active !== null && opensSubmenu(level.entries[level.active])) openChild(depth, level.active, true);
        else moveTop(top, 1, true);
        break;
      case "ArrowLeft":
        handled();
        if (depth > 0) setLevels((current) => current.slice(0, depth));
        else moveTop(top, -1, true);
        break;
      case "Enter":
      case " ":
        handled();
        if (level.active !== null) activate(depth, level.active);
        break;
      case "Escape":
        handled();
        if (depth > 0) setLevels((current) => current.slice(0, depth));
        else close(true);
        break;
      case "Tab":
        handled();
        close(true);
        break;
    }
  };

  return (
    <>
      <div className="menubar-row">
      <div className="menubar" role="menubar" aria-label={label} ref={strip} data-testid="menubar">
        {menus.map((menu, index) => (
          <button
            key={menu.id}
            type="button"
            role="menuitem"
            className="menubar-item"
            data-menu-id={menu.id}
            aria-haspopup="menu"
            aria-expanded={top === index}
            tabIndex={index === focusTop ? 0 : -1}
            title={menu.key === null ? menu.text : undefined}
            ref={(element) => {
              buttons.current[index] = element;
            }}
            onClick={() => (top === index ? close(false) : openTop(index, false))}
            onMouseEnter={() => {
              if (top !== null && top !== index) openTop(index, false);
            }}
            onKeyDown={(event) => onButtonKeyDown(event, index)}
          >
            {labelOf(menu, labels)}
          </button>
        ))}
      </div>
      {tray ? <div className="menubar-tray">{tray}</div> : null}
      </div>
      {top !== null
        ? createPortal(
            <div className="menu-layer" ref={layer} onKeyDown={onPopupKeyDown}>
              {levels.map((level, depthIndex) => (
                <div
                  key={depthIndex}
                  className="menu-popup"
                  role="menu"
                  tabIndex={-1}
                  aria-activedescendant={level.active === null ? undefined : `${uid}-${depthIndex}-${level.active}`}
                  data-testid="menu-popup"
                  data-depth={depthIndex}
                  data-placed={level.placed}
                  style={{ left: level.x, top: level.y }}
                  ref={(element) => {
                    popups.current[depthIndex] = element;
                  }}
                  onMouseEnter={() => {
                    // Reaching a submenu keeps it open, with its own row lit in the parent.
                    clearTimer();
                    if (level.from !== undefined) highlight(depthIndex - 1, level.from);
                  }}
                >
                  {level.entries.map((entry, index) =>
                    entry.kind === "separator" ? (
                      <div key={`sep-${index}`} className="menu-separator" role="separator" />
                    ) : (
                      <MenuRow
                        key={`${entry.id}-${index}`}
                        id={`${uid}-${depthIndex}-${index}`}
                        entry={entry}
                        text={labelOf(entry, labels)}
                        enabled={selectable(entry, bindings)}
                        checked={entry.kind === "check" && bindings[entry.id]?.checked === true}
                        active={level.active === index}
                        expanded={levels[depthIndex + 1] !== undefined && level.active === index}
                        register={(element) => {
                          if (element) rows.current.set(`${depthIndex}:${index}`, element);
                          else rows.current.delete(`${depthIndex}:${index}`);
                        }}
                        onHover={() => hoverRow(depthIndex, index)}
                        onActivate={() => activate(depthIndex, index)}
                      />
                    ),
                  )}
                </div>
              ))}
            </div>,
            document.body,
          )
        : null}
    </>
  );
}

interface MenuRowProps {
  id: string;
  entry: Exclude<MenuEntry, { kind: "separator" }>;
  text: string;
  enabled: boolean;
  checked: boolean;
  active: boolean;
  expanded: boolean;
  register(element: HTMLDivElement | null): void;
  onHover(): void;
  onActivate(): void;
}

function MenuRow({ id, entry, text, enabled, checked, active, expanded, register, onHover, onActivate }: MenuRowProps) {
  const submenu = entry.kind === "submenu";
  const { label, hint } = splitHint(text);
  return (
    <div
      id={id}
      ref={register}
      className="menu-row"
      role={entry.kind === "check" ? "menuitemcheckbox" : "menuitem"}
      aria-checked={entry.kind === "check" ? checked : undefined}
      aria-disabled={enabled ? undefined : true}
      aria-haspopup={submenu ? "menu" : undefined}
      aria-expanded={submenu ? expanded : undefined}
      data-menu-id={entry.id}
      data-active={active || undefined}
      onMouseEnter={onHover}
      onClick={onActivate}
    >
      <span className="menu-mark" aria-hidden="true">
        {checked ? (
          <svg viewBox="0 0 12 12" width="12" height="12">
            <path d="M2.4 6.4 4.9 8.8 9.6 3.4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : null}
      </span>
      <span className="menu-label">{label}</span>
      {hint ? <span className="menu-hint">{hint}</span> : null}
      <span className="menu-chevron" aria-hidden="true">
        {submenu ? (
          <svg viewBox="0 0 8 12" width="8" height="12">
            <path d="M2.5 2.5 6 6l-3.5 3.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : null}
      </span>
    </div>
  );
}
