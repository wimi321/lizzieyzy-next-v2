import { defineConfig } from "vitest/config";

// Only the DOM observer helpers run in jsdom. The desktop scenarios use
// tauri-driver and the production backend, with no Vite mock alias.
export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["e2e/tauri/*.test.ts"],
    testTimeout: 5000,
    hookTimeout: 5000,
  },
});
