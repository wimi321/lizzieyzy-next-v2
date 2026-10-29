import { expect, test } from "@playwright/test";

test("opens the home page", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await expect(page).toHaveTitle("LizzieYzy Next 2 (dev)");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("LizzieYzy Next 2 (dev)");
  await expect(page.getByRole("main")).toHaveText("LizzieYzy Next 2 (dev)");
  expect(errors).toEqual([]);
});
