import { describe, expect, test } from "vitest";
import { analysisUpdated, documentChanged, midGame, midGameAnalysis, nineByNine } from "../../e2e/fixtures";
import { createStore, initialState, reduce } from "./store";

describe("store", () => {
  test("keeps only analysis of the current node at the current revision", () => {
    let state = reduce(initialState, documentChanged(midGame(), 5));
    state = reduce(state, analysisUpdated(midGameAnalysis(4, 20)));
    expect(state.analysis).toBeNull();
    state = reduce(state, analysisUpdated(midGameAnalysis(5, 19)));
    expect(state.analysis).toBeNull();
    state = reduce(state, analysisUpdated(midGameAnalysis(5, 20)));
    expect(state.analysis?.revision).toBe(5);
  });

  test("a new revision drops the old analysis", () => {
    let state = reduce(initialState, documentChanged(midGame(), 5));
    state = reduce(state, analysisUpdated(midGameAnalysis(5, 20)));
    state = reduce(state, documentChanged(nineByNine(), 6));
    expect(state.analysis).toBeNull();
    expect(state.document?.board?.width).toBe(9);
  });

  test("an older document event cannot roll the view back", () => {
    let state = reduce(initialState, documentChanged(nineByNine(), 6));
    state = reduce(state, documentChanged(midGame(), 5));
    expect(state.revision).toBe(6);
    expect(state.document?.board?.width).toBe(9);
  });

  test("notifies listeners only when the state changes", () => {
    const store = createStore();
    let notified = 0;
    const unsubscribe = store.subscribe(() => notified++);
    store.dispatch(documentChanged(midGame(), 5));
    store.dispatch(analysisUpdated(midGameAnalysis(4, 20)));
    store.dispatch({ type: "EngineStatusChanged", data: { type: "Starting" } });
    expect(notified).toBe(2);
    unsubscribe();
    store.dispatch({ type: "EngineStatusChanged", data: { type: "Stopped" } });
    expect(notified).toBe(2);
  });
});
