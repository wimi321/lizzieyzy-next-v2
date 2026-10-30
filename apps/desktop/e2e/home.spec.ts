import { expect, test } from "@playwright/test";

test("opens the application shell without errors", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await expect(page).toHaveTitle("LizzieYzy Next 2");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("LizzieYzy Next 2");
  // No document has arrived from the (silent) mock backend yet.
  await expect(page.getByTestId("position-error")).toBeVisible();
  await expect(page.getByTestId("engine-status")).toHaveAttribute("data-state", "Stopped");
  expect(errors).toEqual([]);
});
