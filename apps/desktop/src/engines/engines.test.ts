import { expect, test } from "vitest";
import type { Settings } from "@/api/backend";
import { describe, ENGINE_DEFAULTS, engineName, engineState, nameFromModel } from "./model";

const profile = { program: "/opt/katago", model: "/opt/kata1-b18c384nbt.bin.gz", config: "", extra_args: [] };

test("the engine list, the main engine and the options come from the settings, with Java's defaults filled in", () => {
  const settings = { version: 1, engine: profile, pondering: true, engines: [{ id: "e1", name: "B18", profile }, { id: "broken" }], main_engine_id: "e1", engine_settings: { read_komi: false } } as Settings;
  const state = engineState(settings);
  expect(state.engines).toEqual([{ id: "e1", name: "B18", profile }]);
  expect(state.main).toBe("e1");
  expect(state.options).toEqual({ ...ENGINE_DEFAULTS, read_komi: false });
  expect(engineState({ version: 1, engine: null, pondering: true } as Settings)).toEqual({ engines: [], main: null, options: ENGINE_DEFAULTS });
});

test("names: a migrated engine takes the model's name while it is the main one; a new one starts from its file", () => {
  const legacy = { id: "legacy-engine", name: "KataGo", profile };
  expect(engineName(legacy, true, "Transformer B11")).toBe("Transformer B11");
  expect(engineName(legacy, false, "Transformer B11")).toBe("KataGo");
  expect(engineName({ ...legacy, name: "Mine" }, true, "Transformer B11")).toBe("Mine");
  expect(nameFromModel("/w/kata1-b18c384nbt.bin.gz")).toBe("kata1-b18c384nbt");
  expect(nameFromModel("C:\\w\\g170e.txt.gz")).toBe("g170e");
});

test("errors read as their reason", () => {
  expect(describe({ type: "App", data: "engine exited" })).toBe("engine exited");
  expect(describe({ type: "Engine", data: { reason: "missing model" } })).toBe("missing model");
  expect(describe(new Error("boom"))).toBe("boom");
});
