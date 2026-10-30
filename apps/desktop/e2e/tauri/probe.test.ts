import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { disconnectProbe, installProbe, readProbe } from "./probe";

function element(id: string): HTMLElement {
  const result = document.querySelector<HTMLElement>(`[data-testid="${id}"]`);
  if (!result) throw new Error(`missing test element: ${id}`);
  return result;
}

function identity(id: string, revision: number, node: number): void {
  const target = element(id);
  target.dataset.revision = String(revision);
  target.dataset.nodeId = String(node);
}

beforeEach(() => {
  document.body.innerHTML = `
    <section data-testid="board-area" data-revision="1" data-node-id="0">
      <div data-testid="board" data-columns="19" data-rows="13" data-cell="24.5"></div>
    </section>
    <strong data-testid="move-number">0</strong>
    <span title="150 visits">
      <strong data-testid="analysis-move" data-revision="1" data-node-id="0">0 · 150</strong>
    </span>
    <span data-testid="engine-status" data-state="Ready" title="1.16.5-fake"></span>`;
});

afterEach(() => {
  disconnectProbe();
  document.body.replaceChildren();
});

describe("real webview DOM probe", () => {
  test("executes when WebDriver serializes functions without their module scope", () => {
    const detachedInstall = new Function(`return (${installProbe.toString()})`)() as typeof installProbe;
    const detachedRead = new Function(`return (${readProbe.toString()})`)() as typeof readProbe;
    const detachedDisconnect = new Function(`return (${disconnectProbe.toString()})`)() as typeof disconnectProbe;
    expect(detachedInstall().current.nodeId).toBe("0");
    expect(detachedRead().analysisVersion).toBe(1);
    detachedDisconnect();
    expect(window.__tauriE2E).toBeUndefined();
  });

  test("keeps a mismatch after a later correct result replaces it between reads", async () => {
    installProbe();
    identity("analysis-move", 99, 8);
    await Promise.resolve();
    identity("analysis-move", 1, 0);
    await Promise.resolve();

    const result = readProbe();
    expect(result.current.analysisRevision).toBe("1");
    expect(result.failures).toEqual([
      "stale analysis at move 0: board=1/0, analysis=99/8 (0 · 150)",
    ]);
    expect(readProbe().failures).toEqual(result.failures);
  });

  test("reads document and analysis identities atomically after a DOM commit", () => {
    const initial = installProbe();
    identity("analysis-move", 2, 1);
    element("move-number").textContent = "1";
    identity("board-area", 2, 1);
    element("analysis-move").textContent = "1 · 150";

    const result = readProbe();
    expect(result.failures).toEqual([]);
    expect(result.observedMoves).toEqual([0, 1]);
    expect(result.analysedMoves).toEqual([0, 1]);
    expect(result.analysisVersion).toBe(initial.analysisVersion + 1);
    expect(initial.observedMoves).toEqual([0]);
  });

  test("does not treat an analysis placeholder as a result or a mismatch", () => {
    installProbe();
    const analysis = element("analysis-move");
    delete analysis.dataset.revision;
    delete analysis.dataset.nodeId;
    analysis.textContent = "Waiting for analysis";
    identity("board-area", 2, 1);
    element("move-number").textContent = "1";

    const result = readProbe();
    expect(result.failures).toEqual([]);
    expect(result.analysisVersion).toBe(1);
    expect(result.analysedMoves).toEqual([0]);
    expect(result.observedMoves).toEqual([0, 1]);
  });

  test("retains a transient Restarting state and requires new analysis after Ready", () => {
    installProbe();
    const engine = element("engine-status");
    engine.dataset.state = "Restarting";
    engine.dataset.state = "Ready";
    const restarted = readProbe();
    expect(restarted.states).toEqual([
      { state: "Ready", analysisVersion: 1, move: 0 },
      { state: "Restarting", analysisVersion: 1, move: 0 },
      { state: "Ready", analysisVersion: 1, move: 0 },
    ]);
    expect(restarted.analysisVersion).toBe(1);

    element("analysis-move").parentElement?.setAttribute("title", "151 visits");
    const recovered = readProbe();
    expect(recovered.analysisVersion).toBe(2);
    expect(recovered.current.analysisTitle).toBe("151 visits");
    expect(recovered.states).toEqual(restarted.states);
    expect(readProbe().analysisVersion).toBe(2);
  });

  test("reports incomplete analysis identity and serializes only DOM observations", () => {
    const initial = installProbe();
    expect(initial.current.board).toEqual({ width: 0, height: 0, cell: 24.5, columns: 19, rows: 13 });
    expect(initial.current.engineDetail).toBe("1.16.5-fake");
    delete element("analysis-move").dataset.nodeId;
    const result = readProbe();
    expect(result.failures).toEqual([
      "stale analysis at move 0: board=1/0, analysis=1/null (0 · 150)",
    ]);
    expect(JSON.parse(JSON.stringify(result))).toEqual(result);
  });

  test("disconnects the observer and rejects reads without an installed probe", () => {
    installProbe();
    disconnectProbe();
    expect(() => readProbe()).toThrow("Tauri DOM probe has not been installed");
  });
});
