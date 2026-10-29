// @vitest-environment node
import { fileURLToPath } from "node:url";
import { expect, test } from "vitest";
import { resolveConfig } from "vite";

test.each([
  ["production", "../src/api/backend.ts", "dist"],
  ["e2e", "../e2e/mock-backend.ts", "dist-e2e"],
])("resolves the %s backend and keeps build output separate", async (mode, modulePath, outDir) => {
  const config = await resolveConfig({ mode }, "build");
  const alias = config.resolve.alias.find((entry) => entry.find === "@/api/backend");
  expect(alias?.replacement).toBe(fileURLToPath(new URL(modulePath, import.meta.url)));
  expect(config.build.outDir).toBe(outDir);
});
