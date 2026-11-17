import { useMemo } from "react";
import { comparisonMenu, engineMenu, type EditingMenuText, menusFor, type MenuEntry, type MenuSubmenu, withDisplayValues, withEditing, withGtpEngines, withJavaImport } from "./spec";

/** 对比引擎 in the menus: the second engine menu while 双引擎模式 is on, and 搜索功能's 选择对比引擎. */
export interface ComparisonMenus {
  /** 双引擎模式 is on: the second engine menu follows the engine menu. */
  shown: boolean;
  /** The comparison engine's place in the engine list, or null. */
  selected: number | null;
  /** 选择对比引擎 in the interface language, and the other words it is found by. */
  search: { text: string; keywords: readonly string[] };
}

/**
 * The menu strip's menus for this platform and this moment: Java's menus with the recent games
 * filled in, then the engine menu. `engine` is the engine's name, "" while it has none yet and
 * null when no engine is configured.
 */
export function useMenus(
  windows: boolean,
  recent: readonly string[],
  text: EditingMenuText,
  engine: string | null,
  rankMarks = 3,
  minVisits = 30,
  /** Every configured engine's menu name, and which one is the main engine. */
  engines: { names: readonly string[]; main: number } | null = null,
  importLabel: string | null = null,
  gtpLabel: string | null = null,
  comparison: ComparisonMenus | null = null,
): readonly MenuSubmenu[] {
  const names = engines?.names.join("\n");
  const main = engines?.main ?? 0;
  const shown = comparison?.shown ?? false;
  const selected = comparison?.selected ?? null;
  const searchText = comparison?.search.text;
  const keywords = comparison?.search.keywords.join("\n");
  return useMemo(() => {
    const java = withDisplayValues(withEditing(menusFor(windows), recent, text), { rankMarks, minVisits });
    const imported = importLabel === null ? java : withJavaImport(java, importLabel);
    const menus = gtpLabel === null ? imported : withGtpEngines(imported, gtpLabel);
    const list = names === undefined ? undefined : names ? names.split("\n") : [];
    const strip = engineMenu(engine, list ?? (engine === null ? [] : [engine]), main);
    // Java's 搜索功能 offers 选择对比引擎 though no menu shows it.
    const search: MenuEntry[] = searchText ? [{ kind: "item", id: "selectComparisonEngine", key: null, text: searchText, keywords: keywords ? keywords.split("\n") : [], searchOnly: true }] : [];
    const second = shown ? [comparisonMenu(list ?? [], selected)] : [];
    return [...menus, { ...strip, items: [...strip.items, ...search] }, ...second];
  }, [windows, recent, text, engine, rankMarks, minVisits, names, main, importLabel, gtpLabel, shown, selected, searchText, keywords]);
}
