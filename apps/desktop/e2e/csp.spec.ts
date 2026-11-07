import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import { analysedGame, analysisUpdated, documentChanged, midGameAnalysis } from "./fixtures";
import type { MockScript } from "./mock-backend";

/**
 * The packaged window's Content-Security-Policy. Tauri serves the app with it; the Vite preview the
 * other specs run against does not, so a picture the policy refuses would pass them unnoticed.
 */
const CSP: string = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8")).app.security.csp;

const ENGINE = { program: "/opt/katago", model: "/opt/kata1-tf3-b11c768-s12002M-d6304M.bin.gz", config: "", extra_args: [] };

test.use({ viewport: { width: 1429, height: 941 }, locale: "zh-CN" });

/** What the policy refused since the page started, kept on the window by an init script. */
type Watched = Window & { __refused?: string[] };

async function startUnderCsp(page: Page) {
  await page.route("**/*", async (route) => {
    if (route.request().resourceType() !== "document") return route.continue();
    const response = await route.fetch();
    await route.fulfill({ response, headers: { ...response.headers(), "content-security-policy": CSP } });
  });
  await page.addInitScript(() => {
    const watched = window as Watched;
    watched.__refused = [];
    document.addEventListener(
      "securitypolicyviolation",
      (event) => watched.__refused?.push(`${event.effectiveDirective} ${event.blockedURI.slice(0, 40)}`),
      true,
    );
  });
  const script: MockScript = {
    initialEvents: [documentChanged(analysedGame(), 7), analysisUpdated(midGameAnalysis(7, 20))],
    commands: [{ command: "settings", result: { version: 1, engine: ENGINE, pondering: true } }],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toBeVisible();
}

/** Pictures on the page that did not load, by the start of their source. */
function unloaded(page: Page) {
  return page
    .locator("img")
    .evaluateAll((images) =>
      images.filter((image) => (image as HTMLImageElement).naturalWidth === 0).map((image) => (image.getAttribute("src") ?? "").slice(0, 40)),
    );
}

test("under the packaged window's CSP every picture loads and nothing is refused", async ({ page }) => {
  await startUnderCsp(page);
  await expect(page.getByTestId("top-toolbar").locator("img").first()).toBeVisible();
  await expect.poll(() => unloaded(page)).toEqual([]);

  // 帮助 → 关于 shows the logo.
  await page.getByTestId("menubar").getByRole("menuitem", { name: "帮助" }).click();
  await page.locator('.menu-row[data-menu-id="about"]').click();
  await expect(page.getByTestId("about-dialog")).toBeVisible();
  await expect.poll(() => unloaded(page)).toEqual([]);

  expect(await page.evaluate(() => (window as Watched).__refused)).toEqual([]);
});
