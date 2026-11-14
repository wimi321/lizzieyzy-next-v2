import { useMemo } from "react";
import { engineMenu, type EditingMenuText, menusFor, type MenuSubmenu, withDisplayValues, withEditing, withGtpEngines, withJavaImport } from "./spec";

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
): readonly MenuSubmenu[] {
  const names = engines?.names.join("\n");
  const main = engines?.main ?? 0;
  return useMemo(() => {
    const java = withDisplayValues(withEditing(menusFor(windows), recent, text), { rankMarks, minVisits });
    const imported = importLabel === null ? java : withJavaImport(java, importLabel);
    const menus = gtpLabel === null ? imported : withGtpEngines(imported, gtpLabel);
    const list = names === undefined ? undefined : names ? names.split("\n") : [];
    return [...menus, engineMenu(engine, list ?? (engine === null ? [] : [engine]), main)];
  }, [windows, recent, text, engine, rankMarks, minVisits, names, main, importLabel, gtpLabel]);
}
