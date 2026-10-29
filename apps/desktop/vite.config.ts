import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  resolve: {
    alias: {
      "@/api/backend": fileURLToPath(
        new URL(
          mode === "e2e" ? "./e2e/mock-backend.ts" : "./src/api/backend.ts",
          import.meta.url,
        ),
      ),
    },
  },
  clearScreen: false,
  server: { host: "localhost", port: 1420, strictPort: true },
  build: { outDir: mode === "e2e" ? "dist-e2e" : "dist" },
}));
