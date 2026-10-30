import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdir, mkdtemp, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { Browser } from "webdriverio";
import { readProbe } from "./probe.js";
import { appCapabilities, openSession, TIMEOUT } from "./session.js";
import { recordWindowsProcesses } from "./diagnostics.js";
import { startWindowsApp } from "./windows-app.js";

export { TIMEOUT } from "./session.js";
export const VISITS_PER_SECOND = 10_000;
const diagnostics = resolve("test-results/tauri");
const suffix = process.platform === "win32" ? ".exe" : "";
const application = resolve(process.env.TAURI_E2E_APP ?? `../../target/debug/lizzieyzy-desktop${suffix}`);
const engine = resolve(process.env.TAURI_E2E_ENGINE ?? `../../target/debug/fake-katago${suffix}`);
const sgf = resolve("e2e/tauri/fixtures/mainline.sgf");
const temporaryDirectories = new Set<string>();
const windowsApps = new Set<ReturnType<typeof startWindowsApp>>();
let appAbort = new AbortController();

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
  appAbort = new AbortController();
  const windows = process.platform === "win32";
  const executable = windows ? process.env.TAURI_E2E_NATIVE_DRIVER_WRAPPER : process.env.TAURI_DRIVER ?? "tauri-driver";
  assert.ok(executable, "Windows E2E requires TAURI_E2E_NATIVE_DRIVER_WRAPPER; use windows-webdriver.ps1");
  // Windows talks directly to msedgedriver through the existing logging adapter.
  // Linux keeps tauri-driver's native-driver translation and launch behavior.
  const args = windows ? ["--port=4444"] : ["--port", "4444", "--native-port", "4445"];
  if (!windows && process.env.TAURI_E2E_NATIVE_DRIVER_WRAPPER) {
    args.push("--native-driver", process.env.TAURI_E2E_NATIVE_DRIVER_WRAPPER);
  }
  const driver = spawn(executable, args, {
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, TAURI_E2E_DIAGNOSTICS: diagnostics },
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
        appAbort.abort();
        try {
          for (const pending of windowsApps) {
            // A failed launch already cleans up before rejecting. A ready app
            // still belongs to us even after WebDriver deletes its session.
            const app = await pending.catch(() => undefined);
            await app?.stop();
          }
          windowsApps.clear();
        } finally {
          await stopDriver(driver);
        }
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
      assert.equal(driver.exitCode, null, `WebDriver exited: ${output}`);
      try {
        const response = await fetch("http://127.0.0.1:4444/status", { signal: AbortSignal.timeout(500) });
        const body = await response.json() as { value?: { ready?: boolean } };
        return response.ok && body.value?.ready === true;
      } catch {
        return false;
      }
    }, `WebDriver did not become ready: ${output}`);
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
  options: { deferFault?: boolean } = {},
): Promise<void> {
  const started = performance.now();
  for (const file of [application, engine, sgf]) assert.ok((await stat(file)).isFile(), `missing file: ${file}`);
  // Spaces intentionally exercise native argument boundaries on both platforms.
  const directory = await mkdtemp(join(tmpdir(), "lizzie e2e "));
  temporaryDirectories.add(directory);
  let browser: Browser | undefined;
  let windowsApp: ReturnType<typeof startWindowsApp> | undefined;
  try {
    const model = join(directory, "placeholder model.bin.gz");
    const config = join(directory, "analysis.cfg");
    const faultMarker = join(directory, "fault claimed");
    if (options.deferFault) await writeFile(faultMarker, "Fault suppressed until the UI observer is ready\n");
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
    const args = [`--config-dir=${directory}`, `--open=${sgf}`];
    let debuggerAddress: string | undefined;
    if (process.platform === "win32") {
      windowsApp = startWindowsApp({ application, args, diagnostics, name, signal: appAbort.signal });
      windowsApps.add(windowsApp);
      debuggerAddress = (await windowsApp).debuggerAddress;
    }
    browser = await openSession(name, appCapabilities(application, args, debuggerAddress));
    // After session creation, restore the normal page-load/script budgets.
    await browser.setTimeout({ implicit: 0, pageLoad: TIMEOUT, script: TIMEOUT });
    await run(browser, { faultMarker });
    console.log(`PASS ${name} (${((performance.now() - started) / 1000).toFixed(2)}s)`);
  } catch (error) {
    console.error(`FAIL ${name}`, error);
    if (process.platform === "win32") {
      await recordWindowsProcesses(diagnostics, name, application).catch((diagnosticError: unknown) => {
        console.error("Process snapshot failed:", diagnosticError);
      });
    }
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
      if (windowsApp) {
        const app = await windowsApp.catch(() => undefined);
        await app?.stop();
        windowsApps.delete(windowsApp);
      }
      await rm(directory, { recursive: true, force: true });
      temporaryDirectories.delete(directory);
    }
  }
}
