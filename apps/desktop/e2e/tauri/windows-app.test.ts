// @vitest-environment node
import { access, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer, type Server, type ServerResponse } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { startWindowsApp, type WindowsApp } from "./windows-app.js";

// This fixture runs as a real child process and serves the CDP handshake. A
// control request lets each test release or reject readiness without sleeping.
const fixtureSource = `
const http = require('node:http');
const fs = require('node:fs');
const { spawn } = require('node:child_process');
const [mode, port, controlPort, marker, ...args] = process.argv.slice(2);
fs.writeFileSync(marker, 'started');
console.log(JSON.stringify({ args }));
console.error('application stderr');
if (mode === 'exit') { console.error('fixture startup failed'); process.exit(17); }
function listen(descendant) { const server = http.createServer((request, response) => {
  if (request.url === '/health') { response.end('child'); return; }
  if (request.url === '/exit') { response.end('exiting', () => process.exit(0)); return; }
  if (request.url !== '/json/version') { response.writeHead(404); response.end(); return; }
  const control = http.request({ hostname: '127.0.0.1', port: controlPort, path: '/probe', method: 'POST' }, (reply) => {
    let body = '';
    reply.on('data', (chunk) => { body += chunk; });
    reply.on('end', () => {
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify(body === 'ready' ? {
        Browser: 'Edg/155.0', 'Protocol-Version': '1.3',
        webSocketDebuggerUrl: 'ws://127.0.0.1:' + port + '/devtools/browser/fixture'
      } : { status: 'not ready' }));
    });
  });
  control.on('error', () => { response.destroy(); });
  control.end(JSON.stringify({ pid: process.pid, args, descendant }));
}); server.listen(Number(port), '127.0.0.1'); }
if (mode === 'tree') {
  spawn(process.execPath, [__filename, 'descendant', port, controlPort, marker, ...args], { stdio: ['ignore', 'ignore', 'ignore', 'ipc'] });
} else listen(mode === 'descendant' ? { pid: process.pid, port: Number(port) } : undefined);
`;

interface Probe { response: ServerResponse; pid: number; args: string[]; descendant?: { pid: number; port: number } }
let directory: string;
let applicationFixture: string;
let marker: string;
let control: Server;
let controlPort: number;
let probes: Probe[];
let receive: ((probe: Probe) => void) | undefined;
let apps: WindowsApp[];
let cancellation: AbortController;
let attempts: Promise<WindowsApp>[];

function launchApp(options: Parameters<typeof startWindowsApp>[0]): Promise<WindowsApp> {
  const signal = options.signal ? AbortSignal.any([cancellation.signal, options.signal]) : cancellation.signal;
  const attempt = startWindowsApp({ ...options, signal });
  attempts.push(attempt);
  void attempt.then((app) => { apps.push(app); }, () => {});
  return attempt;
}

async function listen(server: Server, port = 0): Promise<number> {
  await new Promise<void>((resolve, reject) => {
    const cleanup = () => {
      server.off("error", onError);
      server.off("listening", onListening);
    };
    const onError = (error: unknown) => { cleanup(); reject(error); };
    const onListening = () => { cleanup(); resolve(); };
    server.once("error", onError);
    server.once("listening", onListening);
    try { server.listen(port, "127.0.0.1"); } catch (error) { onError(error); }
  });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("missing fixture port");
  return address.port;
}

async function listenAfterExit(server: Server, port: number): Promise<void> {
  const deadline = performance.now() + 5000;
  let cause: unknown;
  do {
    try {
      await listen(server, port);
      return;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "EADDRINUSE") throw error;
      cause = error;
    }
    const remaining = deadline - performance.now();
    if (remaining <= 0) break;
    // Process exit can precede socket release on Windows. Retry the bind itself
    // so the replacement retains the port as soon as it becomes available.
    await delay(Math.min(25, remaining));
  } while (performance.now() < deadline);
  throw new Error(`Replacement could not bind 127.0.0.1:${port} within 5s`, { cause });
}

async function freePort(): Promise<number> {
  const reservation = createServer();
  const port = await listen(reservation);
  await new Promise<void>((resolve) => reservation.close(() => resolve()));
  return port;
}

async function bindAndRelease(port: number): Promise<void> {
  const reservation = createServer();
  await new Promise<void>((resolve, reject) => {
    reservation.once("error", reject);
    reservation.listen(port, "127.0.0.1", resolve);
  });
  await new Promise<void>((resolve) => reservation.close(() => resolve()));
}

function nextProbe(): Promise<Probe> {
  const probe = probes.shift();
  if (probe) return Promise.resolve(probe);
  return new Promise((resolve) => { receive = resolve; });
}

function options(port: number, mode = "serve") {
  return {
    application: process.execPath,
    args: [applicationFixture, mode, String(port), String(controlPort), marker,
      `--config-dir=${join(directory, "settings with spaces")}`, `--open=${join(directory, "game with spaces.sgf")}`],
    diagnostics: directory, name: "fixture", port,
  };
}

beforeEach(async () => {
  directory = await mkdtemp(join(tmpdir(), "lizzie Windows app test "));
  applicationFixture = join(directory, "application fixture.cjs");
  marker = join(directory, "started marker");
  await writeFile(applicationFixture, fixtureSource);
  probes = [];
  receive = undefined;
  apps = [];
  attempts = [];
  cancellation = new AbortController();
  control = createServer((request, response) => {
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk: string) => { body += chunk; });
    request.on("end", () => {
      const probe = { ...JSON.parse(body) as { pid: number; args: string[] }, response };
      if (receive) {
        const resolve = receive;
        receive = undefined;
        resolve(probe);
      } else probes.push(probe);
    });
  });
  controlPort = await listen(control);
});

afterEach(async () => {
  cancellation.abort();
  await Promise.allSettled(attempts);
  await Promise.all(apps.map((app) => app.stop()));
  control.closeAllConnections();
  await new Promise<void>((resolve) => control.close(() => resolve()));
  vi.restoreAllMocks();
  await rm(directory, { recursive: true, force: true });
});

test("starts without a shell, waits for valid CDP, preserves arguments and logs, and stops idempotently", async () => {
  const port = await freePort();
  const launch = options(port);
  let ready = false;
  const starting = launchApp(launch).then((app) => { ready = true; return app; });
  const first = await nextProbe();
  expect(first.args).toEqual(launch.args.slice(5));
  expect(ready).toBe(false);
  first.response.end("not ready");
  const second = await nextProbe();
  expect(ready).toBe(false);
  second.response.end("ready");
  const app = await starting;
  expect(app.debuggerAddress).toBe(`127.0.0.1:${port}`);
  expect(JSON.parse(await readFile(join(directory, "fixture-cdp-version.json"), "utf8")))
    .toMatchObject({ Browser: "Edg/155.0" });
  await Promise.all([app.stop(), app.stop()]);
  expect(() => process.kill(first.pid, 0)).toThrow();
  expect(await readFile(join(directory, "fixture-application-stdout.log"), "utf8"))
    .toContain(JSON.stringify({ args: launch.args.slice(5) }));
  expect(await readFile(join(directory, "fixture-application-stderr.log"), "utf8"))
    .toContain("application stderr");
  await bindAndRelease(port);
});

test("refuses an occupied debug port without launching or stopping the existing listener", async () => {
  const existing = createServer((_, response) => response.end("existing owner"));
  const port = await listen(existing);
  try {
    await expect(launchApp(options(port))).rejects.toThrow("Refusing to launch: CDP port");
    await expect(access(marker)).rejects.toThrow();
    expect(await (await fetch(`http://127.0.0.1:${port}`)).text()).toBe("existing owner");
  } finally {
    existing.closeAllConnections();
    await new Promise<void>((resolve) => existing.close(() => resolve()));
  }
});

test("reports early application exit with its real status and captures stderr", async () => {
  await expect(launchApp(options(await freePort(), "exit"))).rejects.toThrow("code 17");
  const report = JSON.parse(await readFile(join(directory, "fixture-application.json"), "utf8")) as { exitCode: number; pid: number };
  expect(report.exitCode).toBe(17);
  expect(() => process.kill(report.pid, 0)).toThrow();
  expect(await readFile(join(directory, "fixture-application-stderr.log"), "utf8"))
    .toContain("fixture startup failed");
});

test("reports a missing executable instead of waiting for CDP", async () => {
  await expect(launchApp({ ...options(await freePort()), application: join(directory, "missing.exe") }))
    .rejects.toThrow(/ENOENT/);
});

test("cancelling startup reaps the process before returning", async () => {
  const controller = new AbortController();
  const starting = launchApp({ ...options(await freePort()), signal: controller.signal });
  const rejected = expect(starting).rejects.toThrow(/abort/i);
  const probe = await nextProbe();
  controller.abort();
  await rejected;
  expect(() => process.kill(probe.pid, 0)).toThrow();
});

test("a CDP startup deadline fails and reaps the process even while it serves invalid responses", async () => {
  let now = 0;
  vi.spyOn(performance, "now").mockImplementation(() => now);
  const starting = launchApp(options(await freePort()));
  const rejected = expect(starting).rejects.toThrow("did not become ready within 60s");
  const probe = await nextProbe();
  now = 60_000;
  probe.response.end("not ready");
  await rejected;
  expect(() => process.kill(probe.pid, 0)).toThrow();
});

test("cleanup also terminates an application's child process", async () => {
  const port = await freePort();
  const starting = launchApp(options(port, "tree"));
  const probe = await nextProbe();
  if (!probe.descendant) throw new Error("fixture has no child process");
  expect(await (await fetch(`http://127.0.0.1:${probe.descendant.port}/health`)).text()).toBe("child");
  probe.response.end("ready");
  const app = await starting;
  await app.stop();
  // As with WebView2, this fixture's child owns the CDP socket. stop() must
  // finish releasing it; the test does not retry binds to hide a cleanup race.
  expect(probe.descendant.port).toBe(port);
  await bindAndRelease(probe.descendant.port);
  await bindAndRelease(port);
  expect(JSON.parse(await readFile(join(directory, "fixture-application.json"), "utf8")))
    .toMatchObject({ cdpReady: true, cdpPortReleased: true });
});

test("cleanup reports an occupied former CDP port without killing its new owner", async () => {
  const port = await freePort();
  const starting = launchApp(options(port));
  const probe = await nextProbe();
  probe.response.end("ready");
  const app = await starting;
  await fetch(`http://127.0.0.1:${port}/exit`);
  await expect.poll(() => {
    try { process.kill(probe.pid, 0); return false; } catch { return true; }
  }, { timeout: 1000 }).toBe(true);
  const replacement = createServer((_, response) => response.end("replacement owner"));
  await listenAfterExit(replacement, port);
  try {
    // Advance only the cleanup deadline; socket binding and processes remain real.
    let now = 0;
    vi.spyOn(performance, "now").mockImplementation(() => { now += 1000; return now; });
    await expect(app.stop()).rejects.toThrow("remained occupied after 5s");
    expect(await (await fetch(`http://127.0.0.1:${port}`)).text()).toBe("replacement owner");
    expect(JSON.parse(await readFile(join(directory, "fixture-application.json"), "utf8")))
      .toMatchObject({ cdpReady: true, cdpPortReleased: false });
  } finally {
    // The expected failed stop is idempotent and has already reaped the app.
    apps = apps.filter((active) => active !== app);
    replacement.closeAllConnections();
    await new Promise<void>((resolve) => replacement.close(() => resolve()));
  }
});

test("replacement binding retries occupied ports until release without retaining attempt listeners", async () => {
  const owner = createServer();
  const port = await listen(owner);
  const replacement = createServer((_, response) => response.end("replacement owner"));
  const listening = replacement.listeners("listening");
  let collisions = 0;
  const release = () => { if (++collisions === 2) owner.close(); };
  replacement.on("error", release);
  try {
    await listenAfterExit(replacement, port);
    expect(collisions).toBeGreaterThanOrEqual(2);
    expect(replacement.listeners("error")).toEqual([release]);
    expect(replacement.listeners("listening")).toEqual(listening);
    expect(await (await fetch(`http://127.0.0.1:${port}`)).text()).toBe("replacement owner");
  } finally {
    replacement.closeAllConnections();
    await new Promise<void>((resolve) => replacement.close(() => resolve()));
    await new Promise<void>((resolve) => owner.close(() => resolve()));
  }
});

test("replacement binding fails at its five-second deadline if the port stays occupied", async () => {
  const owner = createServer();
  const port = await listen(owner);
  const replacement = createServer();
  const listening = replacement.listeners("listening");
  const clock = vi.spyOn(performance, "now").mockReturnValue(0);
  replacement.once("error", () => { clock.mockReturnValue(5000); });
  const binding = vi.spyOn(replacement, "listen");
  try {
    await expect(listenAfterExit(replacement, port)).rejects.toMatchObject({
      message: `Replacement could not bind 127.0.0.1:${port} within 5s`,
      cause: { code: "EADDRINUSE" },
    });
    expect(binding).toHaveBeenCalledTimes(1);
    expect(replacement.listeners("error")).toEqual([]);
    expect(replacement.listeners("listening")).toEqual(listening);
    expect(owner.listening).toBe(true);
  } finally {
    await new Promise<void>((resolve) => owner.close(() => resolve()));
  }
});

test("replacement binding fails immediately for errors other than EADDRINUSE", async () => {
  const replacement = createServer();
  const listening = replacement.listeners("listening");
  const binding = vi.spyOn(replacement, "listen");
  await expect(listenAfterExit(replacement, -1)).rejects.toMatchObject({ code: "ERR_SOCKET_BAD_PORT" });
  expect(binding).toHaveBeenCalledTimes(1);
  expect(replacement.listeners("error")).toEqual([]);
  expect(replacement.listeners("listening")).toEqual(listening);
});
