import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { EngineDiscovery, EngineSetupProfile } from "@/api/backend";
import en from "../locales/en.json";
import { EngineSetup } from "./EngineSetup";

afterEach(cleanup);

const FOUND: EngineDiscovery = {
  programs: ["/usr/local/bin/katago"],
  models: ["/models/kata1-b28.bin.gz", "/models/g170.bin.gz"],
  configs: ["/configs/analysis.cfg"],
};

function setup(tryConfigureEngine: (profile: EngineSetupProfile) => Promise<void>) {
  const backend = {
    detectEngines: vi.fn(async () => FOUND),
    pickEngineFile: vi.fn(async () => null),
    tryConfigureEngine: vi.fn(tryConfigureEngine),
  };
  const onConfigured = vi.fn();
  const onClose = vi.fn();
  render(<EngineSetup backend={backend} labels={en.setup} current={null} onConfigured={onConfigured} onClose={onClose} />);
  return { backend, onConfigured, onClose };
}

test("selects the first detected program and model and uses the built-in config", async () => {
  setup(async () => {});
  expect(((await screen.findByDisplayValue(/kata1-b28/)) as HTMLSelectElement).value).toBe("/models/kata1-b28.bin.gz");
  expect((screen.getByTestId("setup-program") as HTMLSelectElement).value).toBe("/usr/local/bin/katago");
  expect((screen.getByTestId("setup-config") as HTMLSelectElement).value).toBe("");
});

test("reports the profile only after the engine reached Ready", async () => {
  const { backend, onConfigured, onClose } = setup(async () => {});
  await screen.findByDisplayValue(/kata1-b28/);
  await act(async () => {
    fireEvent.click(screen.getByTestId("setup-start"));
  });
  const profile = { program: "/usr/local/bin/katago", model: "/models/kata1-b28.bin.gz", config: null, extra_args: [] };
  expect(backend.tryConfigureEngine).toHaveBeenCalledWith(profile);
  expect(onConfigured).toHaveBeenCalledWith(profile);
  expect(onClose).not.toHaveBeenCalled();
});

test("shows the command error text and stays open when the engine fails", async () => {
  const { onConfigured } = setup(async () => {
    throw { type: "App", data: "engine setup failed: missing model" };
  });
  await screen.findByDisplayValue(/kata1-b28/);
  await act(async () => {
    fireEvent.click(screen.getByTestId("setup-start"));
  });
  expect(screen.getByTestId("setup-error").textContent).toContain("engine setup failed: missing model");
  expect(onConfigured).not.toHaveBeenCalled();
  expect((screen.getByTestId("setup-start") as HTMLButtonElement).disabled).toBe(false);
});
