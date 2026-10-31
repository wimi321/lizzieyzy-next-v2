import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { EngineDiscovery, EngineProfile, EngineSetupProfile } from "@/api/backend";
import en from "../locales/en.json";
import { EngineSetup } from "./EngineSetup";

afterEach(cleanup);

const FOUND: EngineDiscovery = {
  programs: ["/usr/local/bin/katago"],
  models: ["/models/kata1-b28.bin.gz", "/models/g170.bin.gz"],
  configs: ["/configs/analysis.cfg"],
};

const SAVED: EngineProfile = {
  program: "/usr/local/bin/katago",
  model: "/models/kata1-b28.bin.gz",
  config: "/configs/analysis.cfg",
  extra_args: ["-override-config", "numSearchThreads=8"],
};

function setup(
  tryConfigureEngine: (profile: EngineSetupProfile) => Promise<void>,
  { current = null, engineRunning = false, notice = null }: { current?: EngineProfile | null; engineRunning?: boolean; notice?: string | null } = {},
) {
  const backend = {
    detectEngines: vi.fn(async () => FOUND),
    pickEngineFile: vi.fn(async () => null),
    tryConfigureEngine: vi.fn(tryConfigureEngine),
  };
  const onConfigured = vi.fn();
  const onClose = vi.fn();
  render(
    <EngineSetup
      backend={backend}
      labels={en.setup}
      current={current}
      engineRunning={engineRunning}
      notice={notice}
      onConfigured={onConfigured}
      onClose={onClose}
    />,
  );
  return { backend, onConfigured, onClose };
}

const startButton = () => screen.getByTestId("setup-start") as HTMLButtonElement;

test("selects the first detected program and model and uses the built-in config", async () => {
  setup(async () => {});
  expect(((await screen.findByDisplayValue(/kata1-b28/)) as HTMLSelectElement).value).toBe("/models/kata1-b28.bin.gz");
  expect((screen.getByTestId("setup-program") as HTMLSelectElement).value).toBe("/usr/local/bin/katago");
  expect((screen.getByTestId("setup-config") as HTMLSelectElement).value).toBe("");
  expect(screen.getByRole("heading").textContent).toBe(en.setup.title);
  expect(startButton().textContent).toBe(en.setup.start);
  expect(startButton().disabled).toBe(false);
});

test("reopened with working settings, Apply stays disabled until something changes", async () => {
  const { backend } = setup(async () => {}, { current: SAVED, engineRunning: true });
  await screen.findByDisplayValue(/analysis\.cfg/);
  expect(screen.getByRole("heading").textContent).toBe(en.setup.titleCurrent);
  expect(screen.getByText(en.setup.introCurrent)).toBeTruthy();
  expect(startButton().textContent).toBe(en.setup.apply);
  expect(startButton().disabled).toBe(true);

  fireEvent.change(screen.getByTestId("setup-model"), { target: { value: "/models/g170.bin.gz" } });
  expect(startButton().disabled).toBe(false);
  await act(async () => {
    fireEvent.click(startButton());
  });
  expect(backend.tryConfigureEngine).toHaveBeenCalledWith({ ...SAVED, model: "/models/g170.bin.gz" });
});

test("the same settings can be applied again when their engine is not running", async () => {
  setup(async () => {}, { current: SAVED, engineRunning: false });
  await screen.findByDisplayValue(/analysis\.cfg/);
  expect(startButton().textContent).toBe(en.setup.apply);
  expect(startButton().disabled).toBe(false);
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
  expect(startButton().disabled).toBe(false);
});

test("opened by a failed automatic setup, it says why until a start is tried", async () => {
  setup(async () => {
    throw { type: "App", data: "model not supported" };
  }, { notice: "No KataGo program was found on this computer." });
  expect((await screen.findByTestId("setup-notice")).textContent).toBe(`${en.setup.autoFailed}No KataGo program was found on this computer.`);
  await act(async () => {});
  expect((screen.getByTestId("setup-program") as HTMLSelectElement).value).toBe(FOUND.programs[0]);
  await act(async () => {
    fireEvent.click(screen.getByTestId("setup-start"));
  });
  expect(screen.queryByTestId("setup-notice")).toBeNull();
  expect(screen.getByTestId("setup-error").textContent).toContain("model not supported");
});
