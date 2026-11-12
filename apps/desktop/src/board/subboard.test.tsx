import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { captureVariation, midGame } from "../../e2e/fixtures";
import { SubBoard } from "./SubBoard";

afterEach(cleanup);

test("the sub-board keeps a position's last line until the next replay arrives, and drops it when the position changes", () => {
  const board = midGame().board!;
  const { replay } = captureVariation(7);
  const { rerender } = render(<SubBoard board={board} node={20} revision={7} variation={null} label="sub" />);
  const moves = () => screen.getByTestId("sub-board").dataset.moves;
  expect(moves()).toBe("0");

  rerender(<SubBoard board={board} node={20} revision={7} variation={replay} label="sub" />);
  expect(moves()).toBe("7");
  // The engine's line changed and its replay is on the way: the last one stays up.
  rerender(<SubBoard board={board} node={20} revision={7} variation={null} label="sub" />);
  expect(moves()).toBe("7");
  const shorter = { ...replay, stones: replay.stones.slice(0, 3) };
  rerender(<SubBoard board={board} node={20} revision={7} variation={shorter} label="sub" />);
  expect(moves()).toBe("3");

  // Another move, or another revision of the game: nothing stale is shown.
  rerender(<SubBoard board={board} node={21} revision={7} variation={null} label="sub" />);
  expect(moves()).toBe("0");
  rerender(<SubBoard board={board} node={20} revision={8} variation={null} label="sub" />);
  expect(moves()).toBe("0");
});
