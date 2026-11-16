import js from "@eslint/js";
import { defineConfig, globalIgnores } from "eslint/config";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default defineConfig([
  globalIgnores(["dist/**", "dist-e2e/**", "node_modules/**", "playwright-report/**", "test-results/**", "src-tauri/**"]),
  {
    files: ["**/*.{ts,tsx}"],
    extends: [js.configs.recommended, tseslint.configs.recommended],
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    extends: [reactHooks.configs.flat.recommended, reactRefresh.configs.vite],
  },
  {
    // 网页旁观's browser page runs outside the app: never Tauri, the desktop backend or the mock.
    files: ["src/web/**/*.{ts,tsx}"],
    // Its tests run in Vitest and use the shared fixtures; Vite never bundles them.
    ignores: ["src/web/**/*.test.{ts,tsx}"],
    rules: {
      "@typescript-eslint/no-restricted-imports": [
        "error",
        {
          paths: [{ name: "@/api/backend", allowTypeImports: true, message: "The web page may use the backend's types only." }],
          patterns: [
            { group: ["@tauri-apps/*"], message: "The web page runs in a plain browser." },
            { group: ["**/e2e/**", "**/mock-backend*"], message: "Test code never reaches the web page." },
            { group: ["../api/*", "!../api/generated"], allowTypeImports: true, message: "Only the API's types; commands need Tauri." },
          ],
        },
      ],
    },
  },
]);
