import { useMemo } from "react";
import { engineMenu, type EditingMenuText, menusFor, type MenuSubmenu, withEditing } from "./spec";

/**
 * The menu strip's menus for this platform and this moment: Java's menus with the recent games
 * filled in, then the engine menu. `engine` is the engine's name, "" while it has none yet and
 * null when no engine is configured.
 */
export function useMenus(windows: boolean, recent: readonly string[], text: EditingMenuText, engine: string | null): readonly MenuSubmenu[] {
  return useMemo(() => [...withEditing(menusFor(windows), recent, text), engineMenu(engine)], [windows, recent, text, engine]);
}
