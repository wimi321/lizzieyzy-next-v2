import { act, renderHook } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { AnalysisView, MoveView, PointView, VariationPreviewView } from "@/api/backend";
import { captureVariation } from "../../e2e/fixtures";
import { useVariationPreview } from "./useVariationPreview";

const D16: PointView = { x: 4, y: 3 };

function backend() {
  const replies: Array<{ node: number; moves: MoveView[]; resolve(view: VariationPreviewView): void }> = [];
  return {
    replies,
    previewVariation: vi.fn(
      (node: number, moves: MoveView[]) => new Promise<VariationPreviewView>((resolve) => replies.push({ node, moves, resolve })),
    ),
  };
}

function hook(fake: ReturnType<typeof backend>, initial: { revision: number; analysis: AnalysisView; target: PointView | null }) {
  return renderHook(({ revision, analysis, target }) => useVariationPreview(fake, 20, revision, analysis, target), {
    initialProps: initial,
  });
}

test("nothing is asked while no candidate is hovered", () => {
  const fake = backend();
  const { result } = hook(fake, { revision: 7, analysis: captureVariation(7).analysis, target: null });
  expect(result.current).toBeNull();
  expect(fake.previewVariation).not.toHaveBeenCalled();
});

test("a hovered candidate's line is replayed by Rust and shown once it answers", async () => {
  const fake = backend();
  const { analysis, moves, replay } = captureVariation(7);
  const { result, rerender } = hook(fake, { revision: 7, analysis, target: D16 });
  expect(fake.previewVariation).toHaveBeenCalledWith(20, moves);
  expect(result.current).toBeNull();

  await act(async () => fake.replies[0]?.resolve(replay));
  expect(result.current).toEqual(replay);

  // A new analysis frame with the same line asks nothing new.
  rerender({ revision: 7, analysis: structuredClone(analysis), target: D16 });
  expect(fake.previewVariation).toHaveBeenCalledTimes(1);

  rerender({ revision: 7, analysis, target: null });
  expect(result.current).toBeNull();
});

test("a reply for an older hover, node or revision is dropped", async () => {
  const fake = backend();
  const { analysis, replay } = captureVariation(7);
  const { result, rerender } = hook(fake, { revision: 7, analysis, target: D16 });
  rerender({ revision: 7, analysis, target: { x: 10, y: 6 } });
  expect(fake.previewVariation).toHaveBeenCalledTimes(2);

  await act(async () => fake.replies[0]?.resolve(replay));
  expect(result.current).toBeNull();

  await act(async () => fake.replies[1]?.resolve({ ...replay, revision: 6 }));
  expect(result.current).toBeNull();
});

test("analysis of another node is not previewed", () => {
  const fake = backend();
  const { analysis } = captureVariation(7);
  renderHook(() => useVariationPreview(fake, 21, 7, analysis, D16));
  expect(fake.previewVariation).not.toHaveBeenCalled();
});
