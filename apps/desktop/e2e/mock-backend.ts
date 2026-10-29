import type { invoke as desktopInvoke } from "../src/api/backend";

// No application commands exist in the scaffold yet. Fail on unexpected calls.
export const invoke: typeof desktopInvoke = (command) =>
  Promise.reject(new Error(`No mock handler for command: ${command}`));
