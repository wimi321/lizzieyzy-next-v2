import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { isEnabled, type MenuBindings } from "../menu/bindings";
import type { MenuSubmenu } from "../menu/spec";
import { getMessages } from "../locales";
import type en from "../locales/en.json";
import { buildCatalog, CATEGORIES, type CatalogEntry, type Category, searchCatalog } from "./catalog";

type Labels = typeof en.research.search;

const ENGLISH = getMessages("en").menu;

export interface FunctionSearchProps {
  menus: readonly MenuSubmenu[];
  bindings: MenuBindings;
  /** Menu labels by legacy key, in the interface language. */
  menuLabels: Readonly<Record<string, string>>;
  labels: Labels;
  onClose(): void;
}

function SearchGlyph() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className="search-glyph">
      <circle cx="6.5" cy="6.5" r="4.6" fill="none" stroke="currentColor" strokeWidth="1.8" />
      <path d="M10 10l4.2 4.2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

/**
 * 搜索功能 (Ctrl/⌘K): every menu entry by name, in the interface language or English. An empty
 * field browses Java's six groups; Enter runs the chosen entry, as its menu item would.
 */
export function FunctionSearch({ menus, bindings, menuLabels, labels, onClose }: FunctionSearchProps) {
  const catalog = useMemo(() => buildCatalog(menus, menuLabels, ENGLISH), [menus, menuLabels]);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category | "all">("all");
  const [selected, setSelected] = useState(0);
  const list = useRef<HTMLUListElement>(null);

  const usable = (entry: CatalogEntry) => isEnabled(bindings[entry.id]);
  const searching = query.trim() !== "";
  const shown = searching ? searchCatalog(catalog, query, usable) : catalog.filter((entry) => category === "all" || entry.category === category);
  const index = Math.max(0, Math.min(selected, shown.length - 1));
  const chosen = shown[index];

  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: "nearest" });
  }, [index, query, category]);

  // Java closes the search before the entry runs, so a window it opens comes up on its own.
  const run = (entry: CatalogEntry | undefined) => {
    if (!entry || !usable(entry)) return;
    onClose();
    bindings[entry.id]?.run?.();
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = index + (event.key === "ArrowDown" ? 1 : -1);
      setSelected(Math.max(0, Math.min(next, shown.length - 1)));
    } else if (event.key === "Enter") {
      // A word still being composed is the input method's.
      if (event.nativeEvent.isComposing || event.keyCode === 229) return;
      event.preventDefault();
      run(chosen);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  };
  const reason = (entry: CatalogEntry) => bindings[entry.id]?.reason ?? (bindings[entry.id]?.run === undefined ? labels.notYet : labels.unavailable);

  return (
    <div className="dialog-backdrop search-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="dialog function-search" role="dialog" aria-modal="true" aria-label={labels.title} data-testid="function-search" onKeyDown={onKeyDown}>
        <div className="search-bar">
          <SearchGlyph />
          <input
            className="text-input search-input"
            role="combobox"
            aria-expanded="true"
            aria-controls="function-search-results"
            aria-activedescendant={chosen ? `search-${chosen.key}` : undefined}
            aria-label={labels.title}
            placeholder={labels.hint}
            autoFocus
            value={query}
            data-testid="search-input"
            onChange={(event) => {
              setQuery(event.target.value);
              setSelected(0);
            }}
          />
          <button type="button" className="secondary-button" onClick={onClose}>
            {labels.cancel}
          </button>
        </div>
        {searching ? null : (
          <div className="search-categories" role="group" aria-label={labels.browse}>
            {(["all", ...CATEGORIES] as const).map((id) => (
              <button
                key={id}
                type="button"
                className="search-category"
                aria-pressed={category === id}
                data-testid={`search-category-${id}`}
                onClick={() => {
                  setCategory(id);
                  setSelected(0);
                }}
              >
                {id === "all" ? labels.all : labels.categories[id]}
              </button>
            ))}
          </div>
        )}
        <p className="search-count" data-testid="search-count">
          {shown.length === 0 ? labels.empty : labels.results.replace("{count}", String(shown.length))}
        </p>
        <ul className="search-results" id="function-search-results" role="listbox" aria-label={labels.title} ref={list}>
          {shown.map((entry, at) => {
            const on = at === index;
            const enabled = usable(entry);
            return (
              <li
                key={entry.key}
                id={`search-${entry.key}`}
                className="search-result"
                role="option"
                aria-selected={on}
                aria-disabled={!enabled}
                data-testid="search-result"
                data-entry={entry.id}
                onMouseMove={() => at !== index && setSelected(at)}
                onClick={() => run(entry)}
              >
                <span className="search-title">{entry.title}</span>
                <span className="search-path">{entry.path.join(" › ")}</span>
                {on && (entry.shortcut || !enabled) ? (
                  <span className="search-detail" data-testid="search-detail">
                    {entry.shortcut ? <kbd>{entry.shortcut}</kbd> : null}
                    {enabled ? null : <span>{reason(entry)}</span>}
                  </span>
                ) : null}
              </li>
            );
          })}
        </ul>
        <footer className="search-keys">{labels.keyboard}</footer>
      </section>
    </div>
  );
}
