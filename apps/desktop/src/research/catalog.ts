import { labelOf, type MenuEntry, type MenuSubmenu } from "../menu/spec";

/** Java's six groups of 搜索功能 (`FunctionCatalog.Category`). */
export const CATEGORIES = ["file", "analysis", "game", "engine", "sync", "view"] as const;
export type Category = (typeof CATEGORIES)[number];

/** Each menu of the strip falls into one group; the engine entries of 设置 go with the engine. */
const CATEGORY_OF: Readonly<Record<string, Category>> = {
  fileMenu: "file",
  editMenu: "file",
  analyzeMenu: "analysis",
  gameMenu: "game",
  engineMenu: "engine",
  engineMenu2: "engine",
  live: "sync",
  viewMenu: "view",
  helpMenu: "view",
  settings: "view",
};
const ENGINE_ENTRIES = new Set(["engineConfig", "remoteCompute", "engineRules", "engineParameters", "autoSetup"]);
/** Rows that only say a list is empty, such as 最近打开 before any game was opened. */
const PLACEHOLDERS = new Set(["recentEmpty"]);

/** One menu entry a search can find and run. */
export interface CatalogEntry {
  /** The menu entry's id, which is also its binding's. */
  id: string;
  /** Unique in the catalog: the ids from the strip down to the entry. */
  key: string;
  /** The label in the interface language, without the shortcut Java writes into it. */
  title: string;
  /** The menus it sits in, from the strip down. */
  path: readonly string[];
  shortcut: string | null;
  category: Category;
  /** Normalized title and path, in the interface language and in English. */
  titles: readonly string[];
  paths: readonly string[];
}

type Labels = Readonly<Record<string, string>>;

/** Java writes the key into the label, as 自动播放(Ctrl+A); a shortcut is one key, maybe with modifiers. */
const SHORTCUT = /\s*[(（]\s*((?:(?:Ctrl|Shift|Alt|Cmd|⌘)\s*\+\s*)*(?:[A-Z0-9]|F\d{1,2}|Space|空格|Enter|Home|End|Del|Delete|Esc|[.,←→↑↓]))\s*[)）]\s*$/i;

export function splitShortcut(label: string): [title: string, shortcut: string | null] {
  const found = SHORTCUT.exec(label);
  return found ? [label.slice(0, found.index).trim(), found[1] ?? null] : [label.trim(), null];
}

/** Width, case and spacing do not matter (NFKC, lower case, one space), as in Java's search. */
export function normalize(text: string): string {
  return text.normalize("NFKC").toLowerCase().replace(/\s+/g, " ").trim();
}

function unique(texts: readonly string[]): string[] {
  return [...new Set(texts.filter((text) => text !== ""))];
}

/**
 * Every entry of the menus, submenus opened down to their items: what 搜索功能 lists. Labels come
 * in the interface language and in English, so either finds an entry.
 */
export function buildCatalog(menus: readonly MenuSubmenu[], labels: Labels, english: Labels): CatalogEntry[] {
  const entries: CatalogEntry[] = [];
  const walk = (items: readonly MenuEntry[], category: Category, path: string[], englishPath: string[], keys: string[]) => {
    for (const entry of items) {
      if (entry.kind === "separator" || PLACEHOLDERS.has(entry.id)) continue;
      const own = labelOf(entry, labels);
      const other = labelOf(entry, english);
      if (entry.kind === "submenu") {
        walk(entry.items, category, [...path, own], [...englishPath, other], [...keys, entry.id]);
        continue;
      }
      const [title, shortcut] = splitShortcut(own);
      entries.push({
        id: entry.id,
        key: [...keys, entry.id].join("/"),
        title,
        path,
        shortcut,
        category: ENGINE_ENTRIES.has(entry.id) ? "engine" : category,
        titles: unique([title, splitShortcut(other)[0], ...(entry.keywords ?? [])].map(normalize)),
        paths: unique([...path, ...englishPath].map(normalize)),
      });
    }
  };
  for (const menu of menus) walk(menu.items, CATEGORY_OF[menu.id] ?? "view", [labelOf(menu, labels)], [labelOf(menu, english)], [menu.id]);
  return entries;
}

/** How well an entry matches, best first: the title itself, the phrase in the title, in the path, every word somewhere. */
export function matchRank(entry: CatalogEntry, query: string): number | null {
  const wanted = normalize(query);
  if (wanted === "") return null;
  if (entry.titles.includes(wanted)) return 0;
  if (entry.titles.some((title) => title.includes(wanted))) return 1;
  if (entry.paths.some((path) => path.includes(wanted))) return 2;
  const fields = [...entry.titles, ...entry.paths];
  return wanted.split(" ").every((word) => fields.some((field) => field.includes(word))) ? 3 : null;
}

/** With a query Java shows at most 50 results. */
export const RESULT_LIMIT = 50;

/**
 * The entries that match, best first; among equals the usable ones, then the shallower ones (自动播放
 * itself before the toolbar switch of the same name), then the menus' order.
 */
export function searchCatalog(entries: readonly CatalogEntry[], query: string, usable: (entry: CatalogEntry) => boolean): CatalogEntry[] {
  const found: { entry: CatalogEntry; rank: number; usable: boolean; order: number }[] = [];
  entries.forEach((entry, order) => {
    const rank = matchRank(entry, query);
    if (rank !== null) found.push({ entry, rank, usable: usable(entry), order });
  });
  found.sort((a, b) => a.rank - b.rank || Number(b.usable) - Number(a.usable) || a.entry.path.length - b.entry.path.length || a.order - b.order);
  return found.slice(0, RESULT_LIMIT).map((match) => match.entry);
}
