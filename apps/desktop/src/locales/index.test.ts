import { describe, expect, expectTypeOf, test } from "vitest";
import { getMessages } from "./index";
import en from "./en.json";
import ja from "./ja.json";
import ko from "./ko.json";
import th from "./th.json";
import zhCN from "./zh-CN.json";
import zhTW from "./zh-TW.json";

function entries(messages: Record<string, unknown>, prefix = ""): [string, string][] {
  return Object.entries(messages).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return typeof value === "string"
      ? [[path, value]]
      : entries(value as Record<string, unknown>, path);
  });
}

function placeholders(value: string): string[] {
  return [...new Set(value.match(/\{[^{}]+\}/g) ?? [])].sort();
}

const english = entries(en);
const translations = { "zh-TW": zhTW, ja, ko, th };

describe.each(Object.entries(translations))("%s messages", (_language, messages) => {
  test("has every English key in the same nested order", () => {
    expect(entries(messages).map(([key]) => key)).toEqual(english.map(([key]) => key));
  });

  test("preserves the placeholder set of every English message", () => {
    const translated = new Map(entries(messages));
    for (const [key, reference] of english) {
      expect(placeholders(translated.get(key) ?? ""), key).toEqual(placeholders(reference));
    }
  });

  test("contains no blank translations", () => {
    expect(entries(messages).filter(([, value]) => value.trim().length === 0)).toEqual([]);
  });

  test("preserves shortcut labels and technical names", () => {
    const translated = new Map(entries(messages));
    for (const [key, reference] of english) {
      const shortcuts = reference.match(
        /\((?:Ctrl|Crtl|Alt|ALT|Shift|Shif)\+[^()]+\)|\((?:Space|Enter|Delete|Home|End|Esc|[A-Z←→,.[\]])\)/g,
      ) ?? [];
      const names = reference.match(
        /\b(?:KataGo|SGF|GIB|LizzieYzy|Lizzie|HumanSL|PDA|WRN|CUDA|TensorRT|NVIDIA|B10|B11)\b/g,
      ) ?? [];
      for (const token of [...shortcuts, ...names]) {
        expect(translated.get(key), key).toContain(token);
      }
    }
    expect(messages.frame.engineNames).toEqual(en.frame.engineNames);
    expect(messages.frame.status.spaceToPause).toContain("Space");
    expect(messages.frame.status.spaceToResume).toContain("Space");
  });
});

test.each<[string, string[]]>([
  ["{version}: {done}/{total}", ["{done}", "{total}", "{version}"]],
  ["{total} {done} {done}", ["{done}", "{total}"]],
  ["{0,number,integer} / {1}", ["{0,number,integer}", "{1}"]],
  ["KataGo (Ctrl+X)", []],
])("extracts complete placeholder tokens from %s", (value, expected) => {
  expect(placeholders(value)).toEqual(expected);
});

describe.each([
  {
    messages: zhTW,
    languages: ["zh-TW", "zh-HK", "zh-Hant", "zh-Hant-TW", "zh-Hant-HK", "ZH-tw", "zh_TW", " zh-HK "],
  },
  {
    messages: zhCN,
    languages: ["zh", "zh-CN", "zh-SG", "zh-MO", "zh-Hans", "zh-Hans-CN", "zh-Hans-TW", "ZH_cn"],
  },
  { messages: ja, languages: ["ja", "ja-JP", "JA-jp", "ja_JP"] },
  { messages: ko, languages: ["ko", "ko-KR", "ko-KP", "KO_kr"] },
  { messages: th, languages: ["th", "th-TH", "TH_th"] },
  {
    messages: en,
    languages: ["en", "en-US", "en-GB", "fr", "de-DE", "", "system", "unknown", "jargon", "korean", "thai", "zhongwen"],
  },
])("language mapping for $languages", ({ messages, languages }) => {
  test.each(languages)("returns the complete bundle for %s", (language) => {
    expect(getMessages(language)).toBe(messages);
  });
});

test("keeps the English message shape as the public return type", () => {
  expectTypeOf(getMessages).returns.toEqualTypeOf<typeof en>();
});
