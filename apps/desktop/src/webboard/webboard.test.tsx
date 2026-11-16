import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { Settings, WebBoardStatus } from "@/api/backend";
import en from "../locales/en.json";
import zh from "../locales/zh-CN.json";
import { labelOf, type MenuEntry, menusFor, withEditing } from "../menu/spec";
import { buildCatalog, searchCatalog } from "../research/catalog";
import { type WebBoardBackend, reasonOf, useWebBoardHost } from "./useWebBoardHost";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  Reflect.deleteProperty(navigator, "clipboard");
});

const labels = en.webBoard;
const STOP_LABEL = "Stop web viewer";
const STOPPED: WebBoardStatus = { running: false, port: null, url: null, addresses: [], viewers: 0, trial: null };
const RUNNING: WebBoardStatus = {
  running: true,
  port: 9998,
  url: "http://192.168.1.5:9998/k3y/",
  addresses: ["http://192.168.1.5:9998/k3y/", "http://10.0.0.7:9998/k3y/"],
  viewers: 2,
  trial: null,
};

function fakeBackend(initial: WebBoardStatus = STOPPED) {
  let listener: ((status: WebBoardStatus) => void) | null = null;
  const backend = {
    subscribeWebBoardChanged: vi.fn((next: (status: WebBoardStatus) => void) => {
      listener = next;
      return Promise.resolve(7);
    }),
    unsubscribe: vi.fn(() => Promise.resolve()),
    webBoardStatus: vi.fn(() => Promise.resolve(initial)),
    startWebBoard: vi.fn(() => Promise.resolve(RUNNING)),
    stopWebBoard: vi.fn(() => Promise.resolve()),
    setWebBoardOptions: vi.fn<(options: { allow_trial: boolean }) => Promise<void>>(() => Promise.resolve()),
    forceEndWebTrial: vi.fn(() => Promise.resolve()),
  } satisfies WebBoardBackend;
  const emit = (status: WebBoardStatus) => act(() => listener?.(status));
  return { backend, emit };
}

const flush = () => act(async () => undefined);

type Host = ReturnType<typeof useWebBoardHost>;

async function mount(backend: WebBoardBackend, options: { settings?: Settings | null; windows?: boolean } = {}) {
  const onTrialStart = vi.fn();
  const host: { current: Host | null } = { current: null };
  function Harness() {
    const value = useWebBoardHost(backend, labels, { settings: options.settings ?? null, windows: options.windows ?? false, stopLabel: STOP_LABEL, onTrialStart });
    host.current = value;
    return <div>{value.chip}</div>;
  }
  const view = render(<Harness />);
  await flush();
  return { ...view, onTrialStart, host: () => host.current! };
}

describe("Rust's refusals as words", () => {
  test("the innermost reason, a variant name, or the message", () => {
    expect(reasonOf({ type: "WebBoard", data: { type: "Bind", data: { reason: "address in use" } } })).toBe("address in use");
    expect(reasonOf({ type: "WebBoard", data: { type: "NotRunning" } })).toBe("NotRunning");
    expect(reasonOf({ type: "Io", data: "disk full" })).toBe("disk full");
    expect(reasonOf(new Error("socket closed"))).toBe("socket closed");
    expect(reasonOf("plain")).toBe("plain");
  });
});

describe("同步 → 网页旁观 in the menu", () => {
  const find = (entries: readonly MenuEntry[], id: string): MenuEntry | undefined => {
    for (const entry of entries) {
      if (entry.kind === "separator") continue;
      if (entry.id === id) return entry;
      const inner = entry.kind === "submenu" ? find(entry.items, id) : undefined;
      if (inner) return inner;
    }
    return undefined;
  };

  test("the row Java lacks reads in the interface language, and 搜索功能 finds every row", () => {
    const menus = withEditing(menusFor(false), [], zh.edit);
    const allow = find(menus, "webBoardAllowTrial");
    expect(allow?.kind).toBe("check");
    expect(allow && allow.kind !== "separator" ? labelOf(allow, zh.menu) : null).toBe("允许旁观者试下");
    const catalog = buildCatalog(menus, zh.menu, en.menu);
    const rows = catalog.filter((entry) => entry.key.includes("webBoardMenu"));
    expect(rows.map((entry) => entry.id)).toEqual(["webBoardToggle", "webBoardCopyUrl", "webBoardAllowTrial", "webBoardExitTrial"]);
    expect(rows.every((entry) => entry.category === "sync")).toBe(true);
    expect(searchCatalog(catalog, "试下", () => true).map((entry) => entry.id)).toEqual(expect.arrayContaining(["webBoardAllowTrial", "webBoardExitTrial"]));
  });
});

describe("同步 → 网页旁观 on the host", () => {
  test("while stopped: no chip, the menu offers to start, and copying waits", async () => {
    const { backend } = fakeBackend();
    const { host } = await mount(backend);
    expect(screen.queryByTestId("web-board-chip")).toBeNull();
    const bindings = host().bindings;
    expect(bindings.webBoardToggle?.label).toBeUndefined();
    expect(bindings.webBoardCopyUrl?.enabled).toBe(false);
    expect(bindings.webBoardExitTrial?.enabled).toBe(false);
    expect(bindings.webBoardAllowTrial?.checked).toBe(true);
    expect(host().trial).toBe(false);
  });

  test("starting opens the panel with every address; the menu row then stops it", async () => {
    const { backend } = fakeBackend();
    const { host } = await mount(backend);
    act(() => host().bindings.webBoardToggle?.run?.());
    await flush();
    expect(backend.startWebBoard).toHaveBeenCalledOnce();
    expect(screen.getByTestId("web-board-chip-open").textContent).toBe("Web viewer · 2");
    expect(screen.getAllByTestId<HTMLInputElement>("web-board-address").map((input) => input.value)).toEqual(RUNNING.addresses);
    expect(screen.getByTestId("web-board-viewers").textContent).toBe("Watching: 2");
    expect(screen.getByTestId("web-board-trial").textContent).toBe(labels.trialNone);
    expect(host().bindings.webBoardToggle?.label).toBe(STOP_LABEL);
    expect(host().bindings.webBoardCopyUrl?.enabled).toBe(true);

    fireEvent.click(screen.getByTestId("web-board-panel-close"));
    expect(screen.queryByTestId("web-board-panel")).toBeNull();
    fireEvent.click(screen.getByTestId("web-board-chip-open"));
    expect(screen.getByTestId("web-board-panel")).toBeTruthy();

    fireEvent.click(screen.getByTestId("web-board-stop"));
    await flush();
    expect(backend.stopWebBoard).toHaveBeenCalledOnce();
    expect(screen.queryByTestId("web-board-chip")).toBeNull();
    expect(host().bindings.webBoardToggle?.label).toBeUndefined();
  });

  test("a viewer already running when the window opens shows its chip, panel closed", async () => {
    const { backend } = fakeBackend(RUNNING);
    await mount(backend);
    expect(screen.getByTestId("web-board-chip")).toBeTruthy();
    expect(screen.queryByTestId("web-board-panel")).toBeNull();
  });

  test("a failed start says why for a few seconds", async () => {
    vi.useFakeTimers();
    const { backend } = fakeBackend();
    backend.startWebBoard.mockRejectedValueOnce({ type: "WebBoard", data: { type: "Bind", data: { reason: "no free port" } } });
    const { host } = await mount(backend);
    act(() => host().bindings.webBoardToggle?.run?.());
    await flush();
    expect(screen.getByTestId("web-board-toast").textContent).toBe("The web viewer did not start: no free port");
    expect(screen.queryByTestId("web-board-chip")).toBeNull();
    act(() => vi.advanceTimersByTime(3000));
    expect(screen.queryByTestId("web-board-toast")).toBeNull();
  });

  test("copying an address, and saying so when the clipboard refuses", async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const { backend } = fakeBackend(RUNNING);
    const { host } = await mount(backend);
    fireEvent.click(screen.getByTestId("web-board-chip-open"));
    fireEvent.click(screen.getAllByTestId("web-board-copy")[1]!);
    await flush();
    expect(writeText).toHaveBeenCalledWith("http://10.0.0.7:9998/k3y/");
    expect(screen.getByTestId("web-board-toast").textContent).toBe(labels.copiedUrl);

    writeText.mockRejectedValueOnce(new Error("denied"));
    act(() => host().bindings.webBoardCopyUrl?.run?.());
    await flush();
    expect(writeText).toHaveBeenLastCalledWith(RUNNING.url);
    expect(screen.getByTestId("web-board-toast").textContent).toBe(labels.copyFailed);
  });

  test("allowing trials follows the settings and is changed through the service", async () => {
    const { backend } = fakeBackend(RUNNING);
    const settings = { web_board: { allow_trial: false } } as Settings;
    const { host } = await mount(backend, { settings });
    fireEvent.click(screen.getByTestId("web-board-chip-open"));
    expect(screen.getByTestId<HTMLInputElement>("web-board-allow").checked).toBe(false);
    expect(host().bindings.webBoardAllowTrial?.checked).toBe(false);
    fireEvent.click(screen.getByTestId("web-board-allow"));
    await flush();
    expect(backend.setWebBoardOptions).toHaveBeenCalledWith({ allow_trial: true });
    expect(screen.getByTestId<HTMLInputElement>("web-board-allow").checked).toBe(true);

    backend.setWebBoardOptions.mockRejectedValueOnce({ type: "WebBoard", data: { type: "NotRunning" } });
    act(() => host().bindings.webBoardAllowTrial?.run?.());
    await flush();
    expect(screen.getByTestId("web-board-toast").textContent).toBe("That did not work: NotRunning");
    expect(host().bindings.webBoardAllowTrial?.checked).toBe(true);
  });

  test("a viewer's trial: the chip says so, setup tools are put down once, and the host can end it", async () => {
    vi.useFakeTimers({ toFake: ["Date", "setTimeout", "clearTimeout", "setInterval", "clearInterval"] });
    vi.setSystemTime(new Date("2026-11-15T10:00:00Z"));
    const { backend, emit } = fakeBackend(RUNNING);
    const { host, onTrialStart } = await mount(backend);
    const since = Date.now() - 5 * 60_000;
    emit({ ...RUNNING, trial: { owner: "c1", since } });
    act(() => vi.advanceTimersByTime(0));
    expect(onTrialStart).toHaveBeenCalledOnce();
    expect(host().trial).toBe(true);
    expect(host().bindings.webBoardExitTrial?.enabled).toBe(true);
    expect(screen.getByTestId("web-board-chip").dataset.trial).toBe("true");
    expect(screen.getByTestId("web-board-chip-open").textContent).toBe(labels.chipTrial);

    fireEvent.click(screen.getByTestId("web-board-chip-open"));
    expect(screen.getByTestId("web-board-trial").textContent).toContain("A viewer has been trying moves for 5 min");
    act(() => vi.advanceTimersByTime(60_000));
    expect(screen.getByTestId("web-board-trial").textContent).toContain("for 6 min");

    emit({ ...RUNNING, viewers: 3, trial: { owner: "c1", since } });
    expect(onTrialStart).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByTestId("web-board-chip-end"));
    fireEvent.click(screen.getByTestId("web-board-force-end"));
    act(() => host().bindings.webBoardExitTrial?.run?.());
    expect(backend.forceEndWebTrial).toHaveBeenCalledTimes(3);

    emit({ ...RUNNING, trial: null });
    expect(host().trial).toBe(false);
    expect(screen.queryByTestId("web-board-chip-end")).toBeNull();
    emit({ ...RUNNING, trial: { owner: "c2", since: Date.now() } });
    expect(onTrialStart).toHaveBeenCalledTimes(2);
  });

  test("the firewall note only on Windows; leaving unsubscribes", async () => {
    const { backend } = fakeBackend(RUNNING);
    const windows = await mount(backend, { windows: true });
    fireEvent.click(screen.getByTestId("web-board-chip-open"));
    expect(screen.getByTestId("web-board-panel").textContent).toContain(labels.firewall);
    windows.unmount();
    expect(backend.unsubscribe).toHaveBeenCalledWith(7);

    await mount(fakeBackend(RUNNING).backend, { windows: false });
    fireEvent.click(screen.getByTestId("web-board-chip-open"));
    expect(screen.getByTestId("web-board-panel").textContent).not.toContain(labels.firewall);
  });
});
