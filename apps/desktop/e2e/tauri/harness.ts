import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdir, mkdtemp, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { remote, type Browser } from "webdriverio";
import { readProbe } from "./probe.js";

export const TIMEOUT = 5_000;
export const VISITS_PER_SECOND = 10_000;
const diagnostics = resolve("test-results/tauri");
const suffix = process.platform === "win32" ? ".exe" : "";
const application = resolve(process.env.TAURI_E2E_APP ?? `../../target/debug/lizzieyzy-desktop${suffix}`);
const engine = resolve(process.env.TAURI_E2E_ENGINE ?? `../../target/debug/fake-katago${suffix}`);
const sgf = resolve("e2e/tauri/fixtures/mainline.sgf");
const temporaryDirectories = new Set<string>();

/** Conditional polling only; the timer bounds failure, never sequences the app. */
async function poll(check: () => Promise<boolean>, message: string): Promise<void> {
  const deadline = Date.now() + TIMEOUT;
  while (!(await check())) {
    assert.ok(Date.now() < deadline, message);
    await new Promise((done) => setTimeout(done, 25));
  }
}

export async function startDriver(): Promise<() => Promise<void>> {
  await mkdir(diagnostics, { recursive: true });
  const executable = process.env.TAURI_DRIVER ?? "tauri-driver";
  const driver = spawn(executable, ["--port", "4444", "--native-port", "4445"], {
    stdio: ["ignore", "pipe", "pipe"],
    // On Unix this lets failure cleanup also reap the native WebDriver child.
    detached: process.platform !== "win32",
  });
  let output = "";
  let spawnError: Error | undefined;
  driver.on("error", (error) => { spawnError = error; });
  for (const stream of [driver.stdout, driver.stderr]) {
    stream?.on("data", (data: Buffer) => { output = (output + data.toString()).slice(-100_000); });
  }
  let cleanupPromise: Promise<void> | undefined;
  const cleanup = (): Promise<void> => {
    cleanupPromise ??= (async () => {
      try {
        await stopDriver(driver);
        for (const directory of temporaryDirectories) await rm(directory, { recursive: true, force: true });
        temporaryDirectories.clear();
      } finally {
        process.off("SIGINT", interrupted);
        process.off("SIGTERM", terminated);
        await writeFile(join(diagnostics, "driver.log"), output);
      }
    })();
    return cleanupPromise;
  };
  // Detached Unix children otherwise outlive a cancelled/terminated Node runner.
  const exitAfterCleanup = (code: number) => {
    void cleanup().then(() => process.exit(code), (error: unknown) => {
      console.error("Driver cleanup failed:", error);
      process.exit(1);
    });
  };
  const interrupted = () => exitAfterCleanup(130);
  const terminated = () => exitAfterCleanup(143);
  process.once("SIGINT", interrupted);
  process.once("SIGTERM", terminated);
  try {
    await poll(async () => {
      if (spawnError) throw spawnError;
      assert.equal(driver.exitCode, null, `tauri-driver exited: ${output}`);
      try {
        const response = await fetch("http://127.0.0.1:4444/status", { signal: AbortSignal.timeout(500) });
        const body = await response.json() as { value?: { ready?: boolean } };
        return response.ok && body.value?.ready === true;
      } catch {
        return false;
      }
    }, `tauri-driver did not become ready: ${output}`);
    return cleanup;
  } catch (error) {
    await cleanup();
    throw error;
  }
}

async function stopDriver(driver: ChildProcess): Promise<void> {
  if (!driver.pid) return;
  if (process.platform === "win32") {
    if (driver.exitCode !== null) return;
    const kill = spawn("taskkill", ["/pid", String(driver.pid), "/t", "/f"], { stdio: "ignore" });
    await poll(async () => kill.exitCode !== null, "taskkill did not finish");
  } else {
    try {
      process.kill(-driver.pid, "SIGTERM");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
    }
    await poll(async () => driver.exitCode !== null || driver.signalCode !== null, "tauri-driver did not exit");
  }
}

export async function withApp(
  name: string,
  extraArgs: string[],
  run: (browser: Browser, context: { faultMarker: string }) => Promise<void>,
): Promise<void> {
  const started = performance.now();
  for (const file of [application, engine, sgf]) assert.ok((await stat(file)).isFile(), `missing file: ${file}`);
  // Spaces intentionally exercise native argument boundaries on both platforms.
  const directory = await mkdtemp(join(tmpdir(), "lizzie e2e "));
  temporaryDirectories.add(directory);
  let browser: Browser | undefined;
  try {
    const model = join(directory, "placeholder model.bin.gz");
    const config = join(directory, "analysis.cfg");
    const faultMarker = join(directory, "fault claimed");
    await writeFile(model, "fake-katago does not load model contents\n");
    await writeFile(config, "reportAnalysisWinratesAs = BLACK\n");
    await writeFile(join(directory, "settings.json"), JSON.stringify({
      version: 1,
      engine: {
        program: engine, model, config,
        extra_args: ["--visits-per-sec", String(VISITS_PER_SECOND), "--fault-once-file", faultMarker, ...extraArgs],
      },
      pondering: true,
    }));
    const capabilities: WebdriverIO.Capabilities & {
      "tauri:options": { application: string; args: string[] };
    } = {
      "wdio:enforceWebDriverClassic": true,
      // Edge WebDriver treats every argument as a Chromium-style switch.
      "tauri:options": { application, args: [`--config-dir=${directory}`, `--open=${sgf}`] },
    };
    browser = await remote({
      hostname: "127.0.0.1",
      port: 4444,
      logLevel: "error",
      connectionRetryCount: 0,
      connectionRetryTimeout: TIMEOUT,
      waitforTimeout: TIMEOUT,
      waitforInterval: 25,
      // tauri-driver selects WebKitGTK or Edge; do not launch a web browser.
      capabilities,
    });
    await browser.setTimeout({ implicit: 0, pageLoad: TIMEOUT, script: TIMEOUT });
    await run(browser, { faultMarker });
    console.log(`PASS ${name} (${((performance.now() - started) / 1000).toFixed(2)}s)`);
  } catch (error) {
    console.error(`FAIL ${name}`, error);
    if (browser) {
      // Diagnostics are best effort and live only in ignored CI artifacts.
      const results = await Promise.allSettled([
        browser.saveScreenshot(join(diagnostics, `${name}.png`)),
        browser.execute(readProbe).then((probe) => writeFile(join(diagnostics, `${name}.json`), JSON.stringify(probe, null, 2))),
      ]);
      for (const result of results) if (result.status === "rejected") console.error("Diagnostic failed:", result.reason);
    }
    throw error;
  } finally {
    try {
      await browser?.deleteSession();
    } finally {
      await rm(directory, { recursive: true, force: true });
      temporaryDirectories.delete(directory);
    }
  }
}
