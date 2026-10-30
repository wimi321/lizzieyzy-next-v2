import { defineConfig } from "vitest/config";

// DOM observers run in jsdom; the session transport tests opt into Node.
// Desktop scenarios use tauri-driver and the production backend, with no mock alias.
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["e2e/tauri/*.test.ts"],
    testTimeout: 5000,
    hookTimeout: 5000,
  },
});
