import { appendFile } from "node:fs/promises";
import { remote, type Browser } from "webdriverio";

export const TIMEOUT = 5_000;
// POST /session includes native app cold start, WebView2 initialization and
// driver attachment. Its initial page load is infrastructure startup, not a
// DOM/event wait; Windows CI needs more than the usual five-second safety net.
export const STARTUP_TIMEOUT = 60_000;

export function appCapabilities(
  application: string,
  args: string[],
  debuggerAddress?: string,
): WebdriverIO.Capabilities {
  if (debuggerAddress) {
    // Attach only: no binary, args or tauri:options may make the driver launch
    // another application. HKLM enables the port on elevated Windows CI hosts.
    return {
      "wdio:enforceWebDriverClassic": true,
      browserName: "webview2",
      "ms:edgeOptions": { debuggerAddress },
    };
  }
  return {
    "wdio:enforceWebDriverClassic": true,
    "tauri:options": { application, args },
  } as WebdriverIO.Capabilities;
}

export async function openSession(
  name: string,
  capabilities: WebdriverIO.Capabilities,
  port = 4444,
): Promise<Browser> {
  const started = performance.now();
  let outcome = "failed";
  try {
    const browser = await remote({
      hostname: "127.0.0.1",
      port,
      logLevel: "error",
      connectionRetryCount: 0,
      connectionRetryTimeout: STARTUP_TIMEOUT,
      waitforTimeout: TIMEOUT,
      waitforInterval: 25,
      // Supply these in POST /session: setting pageLoad afterwards is too late
      // for the initial page. Linux uses tauri-driver; Windows attaches through Edge.
      capabilities: {
        ...capabilities,
        timeouts: { implicit: 0, pageLoad: STARTUP_TIMEOUT, script: TIMEOUT },
      },
    });
    // Subsequent commands must not inherit the larger startup HTTP budget.
    browser.options.connectionRetryTimeout = TIMEOUT;
    outcome = "ready";
    return browser;
  } finally {
    const seconds = ((performance.now() - started) / 1000).toFixed(2);
    const message = `WebDriver session ${name}: ${outcome} in ${seconds}s (startup limit: 60s).`;
    console.log(message);
    if (process.env.GITHUB_STEP_SUMMARY) {
      // A summary I/O error must not hide the session error or leak a session
      // that the caller would otherwise receive and clean up.
      await appendFile(process.env.GITHUB_STEP_SUMMARY, `${message}\n`).catch((error: unknown) => {
        console.error("Could not write WebDriver session timing to job summary:", error);
      });
    }
  }
}
