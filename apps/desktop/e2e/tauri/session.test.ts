// @vitest-environment node
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { openSession } from "./session.js";

interface Request {
  request: IncomingMessage;
  response: ServerResponse;
  body: unknown;
}

let server: Server;
let port: number;
let directory: string;
let summary: string;
let elapsed: number;
let deadlines: { at: number; controller: AbortController }[];
let requests: Request[];
let receive: ((request: Request) => void) | undefined;

function reply(response: ServerResponse, value: unknown) {
  response.writeHead(200, { "Content-Type": "application/json" });
  response.end(JSON.stringify({ value }));
}

function nextRequest(): Promise<Request> {
  const request = requests.shift();
  if (request) return Promise.resolve(request);
  return new Promise((resolve) => { receive = resolve; });
}

// Drive the real WebDriver HTTP client's abort deadlines without waiting in
// wall-clock time. Node's socket I/O and the client's request handling stay real.
function advance(milliseconds: number) {
  elapsed += milliseconds;
  for (const deadline of deadlines) {
    if (deadline.at <= elapsed) {
      deadline.controller.abort(new DOMException("The operation was aborted due to timeout", "TimeoutError"));
    }
  }
}

beforeEach(async () => {
  elapsed = 0;
  deadlines = [];
  requests = [];
  receive = undefined;
  directory = await mkdtemp(join(tmpdir(), "lizzie session test "));
  summary = join(directory, "summary.md");
  vi.stubEnv("GITHUB_STEP_SUMMARY", summary);
  vi.spyOn(performance, "now").mockImplementation(() => elapsed);
  vi.spyOn(AbortSignal, "timeout").mockImplementation((milliseconds) => {
    const controller = new AbortController();
    deadlines.push({ at: elapsed + milliseconds, controller });
    return controller.signal;
  });
  server = createServer((request, response) => {
    // WebdriverIO initializes its context after POST /session and cleans it up
    // through DELETE /session. Neither endpoint represents an app DOM wait.
    if (request.url === "/session/test-session/window") {
      reply(response, "main-window");
      return;
    }
    if (request.method === "DELETE") {
      reply(response, null);
      return;
    }
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk: string) => { body += chunk; });
    request.on("end", () => {
      const captured = { request, response, body: body ? JSON.parse(body) as unknown : null };
      if (receive) {
        const resolve = receive;
        receive = undefined;
        resolve(captured);
      } else requests.push(captured);
    });
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("test server has no TCP port");
  port = address.port;
});

afterEach(async () => {
  server.closeAllConnections();
  await new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  await rm(directory, { recursive: true, force: true });
});

test("allows cold session creation beyond five seconds, reports duration, and restores command deadlines", async () => {
  const connecting = openSession("cold-start", { "wdio:enforceWebDriverClassic": true }, port);
  // Install a rejection handler before advancing the clock: the old five-second
  // startup budget must fail this test rather than become an unhandled rejection.
  const connected = connecting.then((browser) => ({ browser }), (error: unknown) => ({ error }));
  const creation = await nextRequest();
  expect(creation.request.method).toBe("POST");
  expect(creation.request.url).toBe("/session");
  expect(creation.body).toMatchObject({
    capabilities: { alwaysMatch: { timeouts: { implicit: 0, pageLoad: 60_000, script: 5_000 } } },
  });
  advance(7_250);
  reply(creation.response, { sessionId: "test-session", capabilities: { browserName: "MicrosoftEdge" } });
  const result = await connected;
  if (!("browser" in result)) throw result.error;
  const browser = result.browser;
  expect(await readFile(summary, "utf8")).toContain(
    "WebDriver session cold-start: ready in 7.25s (startup limit: 60s).",
  );
  expect(browser.options.waitforTimeout).toBe(5_000);

  const title = browser.getTitle();
  const rejected = expect(title).rejects.toThrow(/timeout/i);
  expect((await nextRequest()).request.url).toBe("/session/test-session/title");
  advance(5_000);
  await rejected;
  await browser.deleteSession();
});

test("aborts an unresponsive session at sixty seconds and reports failed startup duration", async () => {
  const connecting = openSession("hung-start", { "wdio:enforceWebDriverClassic": true }, port);
  const rejected = expect(connecting).rejects.toThrow(/timeout/i);
  expect((await nextRequest()).request.url).toBe("/session");
  advance(60_000);
  await rejected;
  expect(await readFile(summary, "utf8")).toContain(
    "WebDriver session hung-start: failed in 60.00s (startup limit: 60s).",
  );
  expect(requests).toHaveLength(0);
});
