import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { analysisView, candidate, midGame } from "../../e2e/fixtures";
import zh from "../locales/zh-CN.json";
import { CandidateList } from "./CandidateList";

afterEach(cleanup);

const board = midGame().board!;
// White to play, so the mover's numbers are White's: 1 − Black's winrate, minus Black's lead.
const toWhite = { ...board, to_play: "White" as const };
const analysis = analysisView(7, 20, [
  candidate({ x: 10, y: 6 }, 1, 0.4, 300, -1.5, [], toWhite),
  candidate({ x: 3, y: 3 }, 0, 0.3, 900, -3.25, [], toWhite),
]);

function rows() {
  return screen.getAllByTestId("candidate-row").map((row) => within(row).getAllByRole("cell").map((cell) => cell.textContent));
}

test("the candidates in the engine's order, from the mover's side, with each one's share of the visits", () => {
  render(<CandidateList analysis={analysis} toPlay="White" blackWinrate={false} scoreOffset={0} labels={zh.candidates} onHover={vi.fn()} onPlay={vi.fn()} />);
  expect(screen.getAllByRole("columnheader").map((cell) => cell.textContent)).toEqual(["序号", "坐标", "胜率(%)", "计算量", "占比(%)", "目差"]);
  expect(rows()).toEqual([
    ["1", "D16", "70.0", "900", "75.0", "3.3"],
    ["2", "L13", "60.0", "300", "25.0", "1.5"],
  ]);
});

test("总是显示黑方胜率 and the score without komi change the numbers", () => {
  render(<CandidateList analysis={analysis} toPlay="White" blackWinrate scoreOffset={7.5} labels={zh.candidates} onHover={vi.fn()} onPlay={vi.fn()} />);
  expect(rows()[0]).toEqual(["1", "D16", "30.0", "900", "75.0", "4.3"]);
});

test("a row previews its move under the pointer and plays it on a double click; no analysis says so", () => {
  const onHover = vi.fn();
  const onPlay = vi.fn();
  const { rerender } = render(<CandidateList analysis={analysis} toPlay="White" blackWinrate={false} scoreOffset={0} labels={zh.candidates} onHover={onHover} onPlay={onPlay} />);
  const [first] = screen.getAllByTestId("candidate-row");
  fireEvent.mouseEnter(first!);
  fireEvent.doubleClick(first!);
  fireEvent.mouseLeave(screen.getByTestId("candidate-list"));
  expect(onHover.mock.calls).toEqual([[{ x: 3, y: 3 }], [null]]);
  expect(onPlay).toHaveBeenCalledWith({ x: 3, y: 3 });
  rerender(<CandidateList analysis={null} toPlay="White" blackWinrate={false} scoreOffset={0} labels={zh.candidates} onHover={onHover} onPlay={onPlay} />);
  expect(screen.queryAllByTestId("candidate-row")).toHaveLength(0);
  expect(screen.getByTestId("candidate-list").textContent).toContain(zh.candidates.empty);
});
