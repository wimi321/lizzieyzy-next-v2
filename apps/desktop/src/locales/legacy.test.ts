import { describe, expect, test } from "vitest";
import en from "./legacy/en.json";

const bundles = import.meta.glob<Record<string, string>>("./legacy/*.json", {
  eager: true,
  import: "default",
});
const languages = ["en", "en-US", "ja-JP", "ko", "th-TH", "zh-CN", "zh-HK", "zh-TW"];
const english: Record<string, string> = en;
const englishKeys = Object.keys(english).sort();

// Compare complete tokens, including MessageFormat's number format, as sets:
// translations may reorder or repeat an argument without changing its contract.
function placeholders(value: string): string[] {
  return [...new Set(value.match(/\{\d+(?:,[^{}]*)?\}/g) ?? [])].sort();
}

test("includes every Java bundle under its BCP 47 language tag", () => {
  expect(Object.keys(bundles).sort()).toEqual(
    languages.map((language) => `./legacy/${language}.json`).sort(),
  );
});

test.each<[string, string[]]>([
  ["No arguments; {name} is literal text", []],
  ["{1} then {0}, and {1} again", ["{0}", "{1}"]],
  ["{0,number,integer} moves: {1}", ["{0,number,integer}", "{1}"]],
  ["{10} and {2}", ["{10}", "{2}"]],
  ["The engine's result is {0}", ["{0}"]],
  ["Malformed {0", []],
])("extracts the placeholder set from %s", (value, expected) => {
  expect(placeholders(value)).toEqual(expected);
});

describe.each(Object.entries(bundles))("%s", (_path, messages) => {
  test("has the English key set", () => {
    // The current source bundles have no pre-existing key differences to exempt.
    expect(Object.keys(messages).sort()).toEqual(englishKeys);
  });

  test("preserves every English placeholder set", () => {
    const differences = Object.entries(english).flatMap(([key, reference]) => {
      const value = messages[key];
      const expected = placeholders(reference);
      const actual = value === undefined ? undefined : placeholders(value);
      return JSON.stringify(actual) === JSON.stringify(expected)
        ? []
        : [{ key, expected, actual }];
    });
    expect(differences).toEqual([]);
  });
});
