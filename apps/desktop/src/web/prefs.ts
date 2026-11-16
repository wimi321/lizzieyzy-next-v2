import type { CoordinateStyle } from "../board/format";
import en from "../locales/en.json";
import zhCN from "../locales/zh-CN.json";
import zhTW from "../locales/zh-TW.json";

/** The viewer's own choices, kept in this browser (Java's page keeps them in localStorage too). */
export interface WebPrefs {
  coordinates: CoordinateStyle | "off";
  /** 黑方视角: win rates and leads as Black sees them; otherwise as White does. */
  asBlack: boolean;
  winrate: boolean;
  visits: boolean;
  score: boolean;
  ownership: boolean;
}

export const DEFAULT_PREFS: WebPrefs = { coordinates: "standard", asBlack: true, winrate: true, visits: true, score: true, ownership: false };

const KEY = "lizzieyzy-web-board";
const COORDINATES: readonly WebPrefs["coordinates"][] = ["off", "standard", "withI", "fox"];

/** Unknown or damaged values fall back one by one. */
function parseJson(raw: string | null): unknown {
  try {
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

export function parsePrefs(raw: string | null): WebPrefs {
  const value = parseJson(raw);
  const saved = value !== null && typeof value === "object" ? (value as Record<string, unknown>) : {};
  const flag = (key: Exclude<keyof WebPrefs, "coordinates">) => (typeof saved[key] === "boolean" ? (saved[key] as boolean) : DEFAULT_PREFS[key]);
  const coordinates = COORDINATES.find((style) => style === saved.coordinates) ?? DEFAULT_PREFS.coordinates;
  return { coordinates, asBlack: flag("asBlack"), winrate: flag("winrate"), visits: flag("visits"), score: flag("score"), ownership: flag("ownership") };
}

export function loadPrefs(storage: Pick<Storage, "getItem"> | null): WebPrefs {
  try {
    return parsePrefs(storage?.getItem(KEY) ?? null);
  } catch {
    return DEFAULT_PREFS;
  }
}

export function savePrefs(storage: Pick<Storage, "setItem"> | null, prefs: WebPrefs): void {
  try {
    storage?.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // A private window may refuse storage; the choice still applies to this page.
  }
}

export type WebMessages = typeof en;

/** Chinese (simplified or traditional) or English, by the browser's languages; others get English. */
export function messagesFor(languages: readonly string[]): WebMessages {
  for (const language of languages) {
    const tag = language.trim().toLowerCase().replace("_", "-");
    if (!tag.startsWith("zh")) {
      if (tag.startsWith("en")) return en;
      continue;
    }
    return /^zh-(tw|hk|mo|hant)/.test(tag) ? zhTW : zhCN;
  }
  return en;
}
