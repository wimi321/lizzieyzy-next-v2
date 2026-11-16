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
  // Relative asset URLs: 网页旁观 serves the build under /<token>/, the app at its root.
  base: "./",
  clearScreen: false,
  server: { host: "localhost", port: 1420, strictPort: true },
  build: {
    outDir: mode === "e2e" ? "dist-e2e" : "dist",
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        web: fileURLToPath(new URL("./web.html", import.meta.url)),
      },
    },
  },
}));
