import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import zh from "../locales/zh-CN.json";
import { useClearing } from "./useClearing";

afterEach(cleanup);

function Harness({ backend }: { backend: { clearAnalysis: (all: boolean) => Promise<number>; clearCache: (all: boolean) => Promise<void> } }) {
  const clearing = useClearing(backend, true, zh.clearing);
  return (
    <>
      {Object.entries(clearing.bindings).map(([id, binding]) => (
        <button key={id} data-testid={id} onClick={() => binding.run?.()} />
      ))}
      {clearing.dialog}
    </>
  );
}

test("this move clears at once; the whole game and the whole cache ask first", () => {
  const backend = { clearAnalysis: vi.fn(() => Promise.resolve(8)), clearCache: vi.fn(() => Promise.resolve()) };
  render(<Harness backend={backend} />);
  fireEvent.click(screen.getByTestId("clearThisLizzieBestmoves"));
  fireEvent.click(screen.getByTestId("clearThisLizzieCache"));
  expect(backend.clearAnalysis.mock.calls).toEqual([[false]]);
  expect(backend.clearCache.mock.calls).toEqual([[false]]);

  fireEvent.click(screen.getByTestId("clearAllLizzieBestmoves"));
  expect(screen.getByTestId("confirm-clear-analysis").textContent).toContain(zh.clearing.analysisAll);
  fireEvent.click(screen.getByTestId("confirm-clear-analysis-cancel"));
  expect(backend.clearAnalysis).toHaveBeenCalledTimes(1);

  fireEvent.click(screen.getByTestId("clearAllLizzieCache"));
  fireEvent.click(screen.getByTestId("confirm-clear-cache-ok"));
  expect(backend.clearCache.mock.calls).toEqual([[false], [true]]);
  expect(screen.queryByTestId("confirm-clear-cache")).toBeNull();
});
