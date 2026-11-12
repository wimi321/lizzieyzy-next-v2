import { useMemo } from "react";
import { engineMenu, type EditingMenuText, menusFor, type MenuSubmenu, withDisplayValues, withEditing } from "./spec";

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
): readonly MenuSubmenu[] {
  return useMemo(
    () => [...withDisplayValues(withEditing(menusFor(windows), recent, text), { rankMarks, minVisits }), engineMenu(engine)],
    [windows, recent, text, engine, rankMarks, minVisits],
  );
}
