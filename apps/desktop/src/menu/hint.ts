/**
 * Java's `MenuShortcutHint`: a label that ends in a shortcut, such as "保存(Ctrl+S)", shows
 * the shortcut right-aligned. Anything else in brackets stays part of the label.
 */
export interface SplitLabel {
  label: string;
  hint: string | null;
}

/** Engine entries ("[1] KataGo") and recent files ("1: game.sgf") show names users chose. */
const USER_CONTENT = /^(?:\[\d+\]|\d+: )/;
const TRAILING = /^([\s\S]*?)\s*[(（]([^()（）]+)[)）]\s*$/;
const MODIFIER = "(?:ctrl|shift|alt|cmd|command|option|meta)";
const KEY =
  "(?:[a-z0-9]|f(?:[1-9]|1[0-2])|home|end|delete|del|enter|space|tab|esc|backspace|insert" +
  "|pgup|pgdn|pageup|pagedown|[←→↑↓,.\\[\\]/\\\\=-]|空格|逗号|逗號|回车|回車)";
const KEYS = new RegExp(`^(?:${MODIFIER}\\s*\\+\\s*)*${KEY}$`, "i");

export function splitHint(text: string): SplitLabel {
  if (USER_CONTENT.test(text)) return { label: text, hint: null };
  const trailing = TRAILING.exec(text);
  const label = trailing?.[1];
  const keys = trailing?.[2]?.trim();
  if (!label?.trim() || !keys || !KEYS.test(keys)) return { label: text, hint: null };
  return { label, hint: normalize(keys) };
}

/** "ALT+R", "Alt + O" and "ctrl+s" read as "Alt+R", "Alt+O" and "Ctrl+S". */
function normalize(keys: string): string {
  const parts = keys.split(/\s*\+\s*/);
  return parts
    .map((part, index) => {
      const last = index === parts.length - 1;
      if (!last) return part.charAt(0).toUpperCase() + part.slice(1).toLowerCase();
      return part.length === 1 ? part.toUpperCase() : part;
    })
    .join("+");
}
