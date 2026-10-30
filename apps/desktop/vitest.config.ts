import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  resolve: {
    // Component tests drive the UI through the scripted mock, as Playwright does.
    alias: {
      "@/api/backend": fileURLToPath(new URL("./e2e/mock-backend.ts", import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}", "tests/**/*.test.ts"],
    testTimeout: 5000,
    hookTimeout: 5000,
  },
});
