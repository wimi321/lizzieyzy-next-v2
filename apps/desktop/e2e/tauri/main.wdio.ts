import assert from "node:assert/strict";
import { rm, stat } from "node:fs/promises";
import { Key, type Browser } from "webdriverio";
import { startDriver, TIMEOUT, VISITS_PER_SECOND, withApp } from "./harness.js";
import { installProbe, readProbe, type ProbeState } from "./probe.js";

function healthy(probe: ProbeState) {
  assert.deepEqual(probe.failures, [], "stale analysis appeared in the DOM");
  assert.notEqual(probe.current.engineState, "Failed", probe.current.engineDetail ?? "engine failed");
}

async function until(browser: Browser, predicate: (probe: ProbeState) => boolean, message: string) {
  let last: ProbeState | undefined;
  await browser.waitUntil(async () => {
    last = await browser.execute(readProbe);
    healthy(last);
    return predicate(last);
  }, { timeout: TIMEOUT, interval: 25, timeoutMsg: message }).catch((error: unknown) => {
    throw new Error(`${message}; last DOM: ${JSON.stringify(last)}`, { cause: error });
  });
  assert.ok(last);
  healthy(last);
  return last;
}

function hasAnalysis(probe: ProbeState) {
  const sample = probe.current;
  return sample.analysisRevision !== null && sample.analysisNodeId !== null
    && sample.analysisRevision === sample.revision && sample.analysisNodeId === sample.nodeId;
}

async function ready(browser: Browser) {
  await browser.waitUntil(async () => browser.execute(() => Boolean(
    document.querySelector('[data-testid="board"]') && document.querySelector('[data-testid="move-number"]'),
  )), { timeout: TIMEOUT, interval: 25, timeoutMsg: "startup SGF board did not appear" });
  await browser.execute(installProbe);
  const first = await until(browser, (probe) => probe.current.engineState === "Ready" && hasAnalysis(probe),
    "startup analysis did not appear");
  assert.equal(first.current.move, 0, "startup must open the fixture at its root");
  assert.equal(first.current.board?.columns, 19);
  assert.equal(first.current.board?.rows, 19);
  assert.ok(first.current.board && first.current.board.width > 0 && first.current.board.cell > 0);
  // The label alone does not prove candidates were painted on the canvas.
  await browser.waitUntil(async () => browser.execute(() => {
    const canvas = document.querySelector<HTMLCanvasElement>('[data-testid="board-analysis"]');
    const context = canvas?.getContext("2d");
    if (!canvas || !context || canvas.width === 0 || canvas.height === 0) return false;
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    for (let i = 3; i < pixels.length; i += 4) if (pixels[i] !== 0) return true;
    return false;
  }), { timeout: TIMEOUT, interval: 25, timeoutMsg: "candidate canvas stayed empty" });
  return first;
}

async function navigation(browser: Browser) {
  const initial = await ready(browser);
  // One real keypress per acknowledged document change; never wait for a search
  // to finish between keys. The observer watches all intervening DOM updates.
  // Down is the next move, as in LizzieYzy Next; left and right switch branches.
  for (let move = 1; move <= 20; move++) {
    await browser.keys(Key.ArrowDown);
    await until(browser, (probe) => probe.current.move === move, `ArrowDown ${move} did not advance`);
  }
  const navigated = await until(browser, (probe) => probe.current.move === 20 && hasAnalysis(probe),
    "analysis did not catch up with move 20");
  assert.notEqual(navigated.current.nodeId, initial.current.nodeId);
  assert.ok(Number(navigated.current.revision) > Number(initial.current.revision));
  for (let move = 0; move <= 20; move++) assert.ok(navigated.observedMoves.includes(move), `move ${move} was never shown`);
  assert.ok(navigated.analysedMoves.includes(0) && navigated.analysedMoves.includes(20), "analysis coverage is empty");

  const target = 8;
  const point = await browser.execute((index: number) => {
    const graph = document.querySelector<HTMLElement>('[data-testid="winrate-graph"]');
    const svg = graph?.querySelector("svg");
    if (!graph || !svg) throw new Error("graph has no SVG layout");
    const box = svg.getBoundingClientRect();
    const count = Number(graph.dataset.count);
    const left = Number(graph.dataset.plotLeft);
    const right = Number(graph.dataset.plotRight);
    return { count, x: Math.round(box.left + left + index / (count - 1) * (box.width - left - right)),
      y: Math.round(box.top + box.height / 2) };
  }, target);
  assert.equal(point.count, 25, "fixture must expose root plus 24 moves");
  await browser.action("pointer").move({ x: point.x, y: point.y }).down().up().perform();
  await browser.releaseActions();
  const jumped = await until(browser, (probe) => probe.current.move === target && hasAnalysis(probe),
    "graph click did not navigate and analyse move 8");
  assert.notEqual(jumped.current.nodeId, navigated.current.nodeId);
  assert.ok(Number(jumped.current.revision) > Number(navigated.current.revision));
  console.log(`Observed moves 0..20; graph -> ${target}; analysis identities: ${jumped.analysisVersion}`);
}

const CRASH_AFTER_MS = 3_000;

async function recovery(browser: Browser, { faultMarker }: { faultMarker: string }) {
  const initial = await ready(browser);
  // This is a separate process: its crash clock cannot race the 20-key scenario.
  // The temporary marker makes the timer fault one-shot across engine restarts.
  const restarted = await until(browser, (probe) => {
    const index = probe.states.findIndex((entry) => entry.state === "Restarting");
    return index > 0 && probe.states.slice(0, index).some((entry) => entry.state === "Ready")
      && probe.states.slice(index + 1).some((entry) => entry.state === "Ready");
  }, "engine light did not show Ready -> Restarting -> Ready");
  const index = restarted.states.findIndex((entry) => entry.state === "Restarting");
  const afterRestart = restarted.states.slice(index + 1).find((entry) => entry.state === "Ready");
  assert.ok(afterRestart);
  assert.ok((await stat(faultMarker)).isFile(), "the timed crash did not claim its marker");
  // Wait on search progress, not elapsed time. This depth needs more simulated
  // time than a second crash deadline, so closing just before another crash
  // cannot pass. The unchanged node also verifies replay of the interrupted search.
  const survivingVisits = Math.ceil((CRASH_AFTER_MS + 300) / 1000 * VISITS_PER_SECOND);
  const resumed = await until(browser, (probe) => hasAnalysis(probe)
    && probe.current.nodeId === initial.current.nodeId
    && probe.current.revision === initial.current.revision
    && probe.analysisVersion > afterRestart.analysisVersion
    && (probe.current.analysisVisits ?? 0) >= survivingVisits,
    "interrupted analysis did not resume beyond the next crash deadline");
  assert.equal(resumed.states.filter((entry) => entry.state === "Restarting").length, 1,
    "the fault must occur only once");
  assert.equal(resumed.current.engineState, "Ready");
  // A new position additionally rules out accepting only cached pre-crash data.
  await browser.keys(Key.ArrowDown);
  const recovered = await until(browser, (probe) => probe.current.move === 1 && hasAnalysis(probe)
    && probe.analysisVersion > afterRestart.analysisVersion,
    "restarted engine did not analyse a new position");
  assert.equal(recovered.current.engineState, "Ready");
  assert.equal(recovered.states.filter((entry) => entry.state === "Restarting").length, 1);
  console.log(`Engine states: ${recovered.states.map((entry) => entry.state).join(" -> ")}`);
}

async function windowsRecovery(browser: Browser, { faultMarker }: { faultMarker: string }) {
  const initial = await ready(browser);
  // Startup may query both the empty document and the SGF. The precreated
  // one-shot marker suppresses those faults until the observer is installed;
  // after removing it, this navigation is guaranteed to reach query >= 2.
  assert.ok((await stat(faultMarker)).isFile(), "startup fault suppression marker is missing");
  await rm(faultMarker);
  await browser.keys(Key.ArrowDown);
  const restarted = await until(browser, (probe) => {
    const index = probe.states.findIndex((entry) => entry.state === "Restarting");
    return index > 0 && probe.states.slice(0, index).some((entry) => entry.state === "Ready")
      && probe.states.slice(index + 1).some((entry) => entry.state === "Ready");
  }, "engine light did not show Ready -> Restarting -> Ready after navigation");
  const index = restarted.states.findIndex((entry) => entry.state === "Restarting");
  const afterRestart = restarted.states.slice(index + 1).find((entry) => entry.state === "Ready");
  assert.ok(afterRestart);
  assert.ok((await stat(faultMarker)).isFile(), "the query crash did not claim its marker");
  const resumed = await until(browser, (probe) => probe.current.move === 1 && hasAnalysis(probe)
    && probe.analysisVersion > afterRestart.analysisVersion,
    "restarted engine did not replay the interrupted move 1 query");
  assert.notEqual(resumed.current.nodeId, initial.current.nodeId);
  assert.ok(Number(resumed.current.revision) > Number(initial.current.revision));
  // This is at least query 2 in the restarted engine. Analysis at a new node
  // proves the marker prevents a second crash, without racing a wall clock.
  await browser.keys(Key.ArrowDown);
  const recovered = await until(browser, (probe) => probe.current.move === 2 && hasAnalysis(probe)
    && probe.analysisVersion > resumed.analysisVersion,
    "restarted engine did not analyse move 2 beyond its query crash threshold");
  assert.notEqual(recovered.current.nodeId, resumed.current.nodeId);
  assert.equal(recovered.current.engineState, "Ready");
  assert.equal(recovered.states.filter((entry) => entry.state === "Restarting").length, 1);
  console.log(`Engine states: ${recovered.states.map((entry) => entry.state).join(" -> ")}`);
}

if (process.platform === "darwin") {
  console.log("SKIP tauri-driver E2E: macOS has no official WKWebView WebDriver (P1-12).");
} else {
  assert.ok(process.platform === "linux" || process.platform === "win32", "unsupported WebDriver platform");
  const stop = await startDriver();
  try {
    await withApp("navigation", [], navigation);
    if (process.platform === "win32") {
      await withApp("crash-recovery", ["--crash-after-queries", "2"], windowsRecovery, { deferFault: true });
    } else {
      await withApp("crash-recovery", ["--crash-after-ms", String(CRASH_AFTER_MS)], recovery);
    }
  } finally {
    await stop();
  }
}
