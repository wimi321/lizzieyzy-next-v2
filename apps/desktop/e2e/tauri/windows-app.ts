import { execFile, spawn, type ChildProcess } from "node:child_process";
import { mkdir, open, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { dirname, join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { STARTUP_TIMEOUT, TIMEOUT } from "./session.js";

export const WINDOWS_DEBUG_PORT = 9222;

interface WindowsAppOptions {
  application: string;
  args: string[];
  diagnostics: string;
  name: string;
  port?: number;
  signal?: AbortSignal;
}

export interface WindowsApp {
  debuggerAddress: string;
  stop: () => Promise<void>;
}

async function requireFreePort(port: number): Promise<void> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", (cause) => reject(new Error(
      `Refusing to launch: CDP port 127.0.0.1:${port} is unavailable`, { cause },
    )));
    server.listen({ host: "127.0.0.1", port, exclusive: true }, resolve);
  });
  await new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
}

async function waitForReleasedPort(port: number): Promise<void> {
  const deadline = performance.now() + TIMEOUT;
  while (true) {
    try {
      await requireFreePort(port);
      return;
    } catch (cause) {
      if (performance.now() >= deadline) {
        throw new Error(`Application cleanup failed: CDP port 127.0.0.1:${port} remained occupied after 5s`, { cause });
      }
    }
    await delay(25);
  }
}

function exited(child: ChildProcess): boolean {
  return child.exitCode !== null || child.signalCode !== null;
}

async function bounded<T>(promise: Promise<T>, message: string): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([promise, new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(message)), TIMEOUT);
    })]);
  } finally {
    clearTimeout(timer);
  }
}

async function stopProcess(child: ChildProcess, closed: Promise<void>): Promise<void> {
  if (!child.pid) return;
  if (process.platform === "win32") {
    // Do not act on a PID after our process has exited: Windows may reuse it.
    if (!exited(child)) {
      await new Promise<void>((resolve, reject) => {
        execFile("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
          timeout: TIMEOUT, windowsHide: true,
        }, (error) => error && !exited(child) ? reject(error) : resolve());
      });
    }
  } else {
    // A separate process group permits real-process helper tests on Unix.
    try { process.kill(-child.pid, "SIGKILL"); } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
    }
  }
  await bounded(closed, "Application process did not close after cleanup");
}

function cdpVersion(value: unknown, port: number): value is { webSocketDebuggerUrl: string } {
  if (!value || typeof value !== "object" || !("webSocketDebuggerUrl" in value)
    || typeof value.webSocketDebuggerUrl !== "string") return false;
  try {
    const socket = new URL(value.webSocketDebuggerUrl);
    return socket.protocol === "ws:" && socket.hostname === "127.0.0.1" && socket.port === String(port)
      && socket.pathname.startsWith("/devtools/browser/") && socket.pathname.length > "/devtools/browser/".length;
  } catch { return false; }
}

/** Start the real app ourselves; Edge WebDriver only attaches to this CDP endpoint. */
export async function startWindowsApp(options: WindowsAppOptions): Promise<WindowsApp> {
  const { application, args, diagnostics, name, signal } = options;
  const port = options.port ?? WINDOWS_DEBUG_PORT;
  signal?.throwIfAborted();
  await requireFreePort(port);
  await mkdir(diagnostics, { recursive: true });
  signal?.throwIfAborted();

  const stdout = await open(join(diagnostics, `${name}-application-stdout.log`), "w");
  const stderr = await open(join(diagnostics, `${name}-application-stderr.log`), "w").catch(async (error: unknown) => {
    await stdout.close();
    throw error;
  });
  let child: ChildProcess;
  try {
    signal?.throwIfAborted();
    child = spawn(application, args, {
      cwd: dirname(application),
      stdio: ["ignore", stdout.fd, stderr.fd],
      detached: process.platform !== "win32",
      shell: false,
    });
  } catch (error) {
    await Promise.all([stdout.close(), stderr.close()]);
    throw error;
  }
  let spawnError: Error | undefined;
  child.on("error", (error) => { spawnError = error; });
  const closed = new Promise<void>((resolve) => child.once("close", () => resolve()));
  const cancellation = new AbortController();
  let stopPromise: Promise<void> | undefined;
  let cdpReady = false;
  let cdpPortReleased = false;
  const report = {
    startedAt: new Date().toISOString(), application, args, pid: child.pid,
    debuggerAddress: `127.0.0.1:${port}`,
  };
  const stop = (): Promise<void> => {
    stopPromise ??= (async () => {
      signal?.removeEventListener("abort", onAbort);
      cancellation.abort();
      try {
        await stopProcess(child, closed);
        // WebView2 owns this socket in a child process. The host exiting does
        // not establish that its browser has released the port for the next
        // scenario. Only observe availability here; never kill a port owner.
        if (cdpReady) {
          await waitForReleasedPort(port);
          cdpPortReleased = true;
        }
      } finally {
        await writeFile(join(diagnostics, `${name}-application.json`), JSON.stringify({
          ...report, finishedAt: new Date().toISOString(), exitCode: child.exitCode,
          signal: child.signalCode, spawnError: spawnError?.message,
          cdpReady, cdpPortReleased: cdpReady ? cdpPortReleased : null,
        }, null, 2));
      }
    })();
    return stopPromise;
  };
  // Startup also owns the process when the caller has not yet received stop().
  const onAbort = () => { void stop().catch(() => {}); };
  try {
    await Promise.all([stdout.close(), stderr.close()]);
    await writeFile(join(diagnostics, `${name}-application.json`), JSON.stringify(report, null, 2));
    signal?.addEventListener("abort", onAbort, { once: true });
    const requestSignal = signal ? AbortSignal.any([signal, cancellation.signal]) : cancellation.signal;
    // Cold WebView2 initialization is infrastructure startup, like POST /session.
    // Ordinary DOM waits and process cleanup retain the five-second limit.
    const deadline = performance.now() + STARTUP_TIMEOUT;
    let lastError: unknown;
    while (true) {
      requestSignal.throwIfAborted();
      if (spawnError) throw spawnError;
      if (exited(child)) throw new Error(`Application exited before CDP became ready (code ${child.exitCode}, signal ${child.signalCode})`);
      if (performance.now() >= deadline) throw new Error("Application CDP endpoint did not become ready within 60s", { cause: lastError });
      let version: unknown;
      try {
        const response = await fetch(`http://${report.debuggerAddress}/json/version`, {
          signal: AbortSignal.any([requestSignal, AbortSignal.timeout(500)]),
        });
        version = await response.json();
        if (!response.ok || !cdpVersion(version, port)) throw new Error("CDP endpoint returned an invalid browser version");
      } catch (error) {
        lastError = error;
        await delay(25, undefined, { signal: requestSignal });
        continue;
      }
      cdpReady = true;
      requestSignal.throwIfAborted();
      if (exited(child)) throw new Error("Application exited during CDP initialization");
      await writeFile(join(diagnostics, `${name}-cdp-version.json`), JSON.stringify(version, null, 2));
      // Cancellation may arrive while the artifact is being written.
      requestSignal.throwIfAborted();
      if (exited(child)) throw new Error("Application exited during CDP initialization");
      return { debuggerAddress: report.debuggerAddress, stop };
    }
  } catch (error) {
    await stop();
    throw error;
  }
}
