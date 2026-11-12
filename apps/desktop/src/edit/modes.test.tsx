import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { DocumentView } from "@/api/backend";
import { emptyNineteen, midGame } from "../../e2e/fixtures";
import { markColor, markSize } from "../board/markup";
import zh from "../locales/zh-CN.json";
import { clickCommand, useBoardModes } from "./useBoardModes";

afterEach(cleanup);

function fake() {
  return {
    setSetupStone: vi.fn(() => Promise.resolve(1)),
    clearSetup: vi.fn(() => Promise.resolve(1)),
    setFirstPlayer: vi.fn(() => Promise.resolve(1)),
    convertToStartPosition: vi.fn(() => Promise.resolve(1)),
    setMarkup: vi.fn(() => Promise.resolve(1)),
    clearMarkup: vi.fn(() => Promise.resolve(1)),
    toggleToPlay: vi.fn(() => Promise.resolve(1)),
  };
}

const D4 = { x: 3, y: 15 };

test("a click goes to the tool in hand, or to play", () => {
  expect(clickCommand({ kind: "play" }, D4)).toBeNull();
  expect(clickCommand({ kind: "setup", tool: "White" }, D4)).toEqual({ setup: [D4, "White"] });
  expect(clickCommand({ kind: "setup", tool: "Erase" }, D4)).toEqual({ setup: [D4, null] });
  expect(clickCommand({ kind: "markup", tool: "Triangle" }, D4)).toEqual({ markup: [D4, "Triangle"] });
  // The markup tools are out but none is in hand: the board plays.
  expect(clickCommand({ kind: "markup", tool: null }, D4)).toBeNull();
});

test("marks are white on black stones and black elsewhere, sized from the stone as in Java", () => {
  expect(markColor("Black")).toBe("#fff");
  expect(markColor("White")).toBe("#000");
  expect(markColor(null)).toBe("#000");
  expect(markSize("Triangle", 19)).toBe(13);
  expect(markSize("Circle", 19)).toBe(12);
  expect(markSize("Square", 19)).toBe(10);
  expect(markSize("Cross", 19)).toBe(10);
});

describe("the board's editing modes", () => {
  function Chip({ backend, view }: { backend: ReturnType<typeof fake>; view: DocumentView }) {
    const modes = useBoardModes(backend, view, zh.modes);
    return (
      <>
        <output data-testid="mode">{JSON.stringify(modes.mode)}</output>
        <button data-testid="take-black" onClick={() => modes.setupTool("Black")} />
        <button data-testid="markup" onClick={modes.toggleMarkup} />
        <button data-testid="click" onClick={() => modes.onPoint(D4)} />
        {modes.chip}
        {modes.dialog}
      </>
    );
  }
  const mode = () => JSON.parse(screen.getByTestId("mode").textContent ?? "null");

  test("an empty game takes up a setup tool at once; every click places or erases, and the chip sets who plays first", () => {
    const backend = fake();
    render(<Chip backend={backend} view={emptyNineteen()} />);
    fireEvent.click(screen.getByTestId("take-black"));
    expect(mode()).toEqual({ kind: "setup", tool: "Black" });
    fireEvent.click(screen.getByTestId("click"));
    expect(backend.setSetupStone).toHaveBeenCalledWith(D4, "Black");
    fireEvent.click(screen.getByTestId("setup-chip-erase"));
    fireEvent.click(screen.getByTestId("click"));
    expect(backend.setSetupStone).toHaveBeenLastCalledWith(D4, null);
    fireEvent.click(screen.getByTestId("setup-white-first"));
    expect(backend.setFirstPlayer).toHaveBeenCalledWith("White");
    fireEvent.click(screen.getByTestId("setup-clear"));
    expect(backend.clearSetup).toHaveBeenCalledTimes(1);
    // Esc puts the tool down.
    fireEvent.keyDown(window, { key: "Escape" });
    expect(mode()).toEqual({ kind: "play" });
    expect(screen.queryByTestId("setup-chip")).toBeNull();
  });

  test("a game with moves is made a starting position first, after Java's question", async () => {
    const backend = fake();
    render(<Chip backend={backend} view={midGame()} />);
    fireEvent.click(screen.getByTestId("take-black"));
    expect(mode()).toEqual({ kind: "play" });
    expect(screen.getByTestId("confirm-convert").textContent).toContain(zh.modes.convertMessage);
    // Not a question that can be switched off.
    expect(screen.queryByTestId("confirm-convert-no-again")).toBeNull();
    fireEvent.click(screen.getByTestId("confirm-convert-cancel"));
    expect(backend.convertToStartPosition).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("take-black"));
    await act(async () => fireEvent.click(screen.getByTestId("confirm-convert-ok")));
    expect(backend.convertToStartPosition).toHaveBeenCalledTimes(1);
    expect(mode()).toEqual({ kind: "setup", tool: "Black" });
  });

  test("the markup tools toggle as Java's buttons do", () => {
    const backend = fake();
    render(<Chip backend={backend} view={midGame()} />);
    fireEvent.click(screen.getByTestId("markup"));
    expect(mode()).toEqual({ kind: "markup", tool: null });
    fireEvent.click(screen.getByTestId("markup-chip-letter"));
    fireEvent.click(screen.getByTestId("click"));
    expect(backend.setMarkup).toHaveBeenCalledWith(D4, "Letter");
    expect(screen.getByTestId("markup-chip-letter").getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByTestId("markup-chip-letter"));
    expect(mode()).toEqual({ kind: "markup", tool: null });
    fireEvent.click(screen.getByTestId("markup-clear"));
    expect(backend.clearMarkup).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId("markup-chip-done"));
    expect(mode()).toEqual({ kind: "play" });
  });

  test("a game that goes away puts the tool down", () => {
    const backend = fake();
    const { result, rerender } = renderHook(({ view }) => useBoardModes(backend, view, zh.modes), { initialProps: { view: emptyNineteen() as DocumentView | null } });
    act(() => result.current.setupTool("White"));
    expect(result.current.editing).toBe(true);
    rerender({ view: null });
    expect(result.current.mode).toEqual({ kind: "play" });
  });
});
