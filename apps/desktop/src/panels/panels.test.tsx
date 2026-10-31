import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { BoardView, GraphPointView, VariationTreeView } from "@/api/backend";
import en from "../locales/en.json";
import zh from "../locales/zh-CN.json";
import { InfoPanel } from "./InfoPanel";
import { MoveTree } from "./MoveTree";

afterEach(cleanup);

const tree: VariationTreeView = {
  revision: 3,
  root: 0,
  nodes: [
    { id: 0, parent: null, children: [1], move_number: 0, mv: null, move_error: null },
    { id: 1, parent: 0, children: [2, 4], move_number: 1, mv: { type: "Play", data: { color: "Black", point: { x: 15, y: 3 } } }, move_error: null },
    { id: 2, parent: 1, children: [3], move_number: 2, mv: { type: "Play", data: { color: "White", point: { x: 3, y: 15 } } }, move_error: null },
    { id: 3, parent: 2, children: [], move_number: 3, mv: { type: "Pass", data: { color: "Black" } }, move_error: null },
    { id: 4, parent: 1, children: [], move_number: 2, mv: { type: "Play", data: { color: "White", point: { x: 2, y: 2 } } }, move_error: null },
  ],
};

test("the move tree draws every node, marks the current one and jumps on click", () => {
  const onJump = vi.fn();
  const { container } = render(
    <MoveTree tree={tree} current={2} line={[0, 1, 2, 3]} boardSize={19} variationCount={1} labels={zh.tree} onJump={onJump} />,
  );
  const nodes = screen.getAllByTestId("tree-node");
  expect(nodes.map((node) => node.dataset.id)).toEqual(["0", "1", "2", "3", "4"]);
  expect(nodes.find((node) => node.dataset.current === "true")?.dataset.id).toBe("2");
  expect(nodes[1]?.querySelector("title")?.textContent).toBe("1 · Q16");
  // Node 4 is off the displayed line, so its stone is dimmed.
  expect(nodes[4]?.querySelector(".tree-stone")?.classList.contains("off-line")).toBe(true);
  expect(container.querySelectorAll(".tree-edge.lit")).toHaveLength(3);

  fireEvent.click(nodes[4]!);
  expect(onJump).toHaveBeenCalledWith(4);
  fireEvent.click(nodes[2]!);
  expect(onJump).toHaveBeenCalledTimes(1);
});

test("the info panel shows captures, the winrate bar and the last move's change", () => {
  const points: GraphPointView[] = [
    { node_id: 0, move_number: 0, summary: { winrate: 0.5, score_lead: 0.5, visits: 10 } },
    { node_id: 1, move_number: 1, summary: { winrate: 0.62, score_lead: 2.4, visits: 10 } },
  ];
  const board = {
    width: 19,
    height: 19,
    stones: [],
    last_move: { type: "Play", data: { color: "Black", point: { x: 3, y: 3 } } },
    to_play: "White",
    move_number: 1,
    captured_black: 2,
    captured_white: 0,
  } as const;
  render(
    <InfoPanel
      board={{ ...board, stones: [] }}
      black={{ name: "柯洁", rank: "9段" }}
      white={{ name: null, rank: null }}
      points={points}
      currentIndex={1}
      labels={zh.info}
    />,
  );

  expect(screen.getByText("提子 2")).toBeTruthy();
  expect(screen.getByText("柯洁")).toBeTruthy();
  expect(screen.getByText("9段")).toBeTruthy();
  // A missing name falls back to the colour.
  expect(screen.getByText("白方")).toBeTruthy();
  expect(screen.getByTestId("winrate-bar").dataset.black).toBe("62.0");
  expect(screen.getByTestId("lead-line").textContent).toBe("黑领先 2.4 目最后一手 (#1)：+12.0% +1.9 目");
});

test("the game's komi, handicap, rules, result and date sit under the players", () => {
  const board: BoardView = {
    width: 19, height: 19, stones: [], last_move: null, to_play: "White", move_number: 0, captured_black: 0, captured_white: 0,
  };
  const game = {
    black_name: "甲", black_rank: null, white_name: "乙", white_rank: null, komi: 7.5, rules: "Chinese",
    handicap: null, result: "B+R", date: "2026-10-28", event: "野狐 十段赛",
  };
  const { rerender } = render(
    <InfoPanel board={board} black={{ name: "甲", rank: null }} white={{ name: "乙", rank: null }} points={[]} currentIndex={0} game={game} labels={zh.info} />,
  );
  expect(screen.getByTestId("game-meta").textContent).toBe("贴 7.5 · 中国规则 · B+R · 2026-10-28");
  expect(screen.getByTestId("game-meta").getAttribute("title")).toBe("野狐 十段赛");

  rerender(
    <InfoPanel
      board={board}
      black={{ name: "甲", rank: null }}
      white={{ name: "乙", rank: null }}
      points={[]}
      currentIndex={0}
      game={{ ...game, komi: 0.5, handicap: 3, rules: "tromp_taylor", result: null, date: null, event: null }}
      labels={en.info}
    />,
  );
  expect(screen.getByTestId("game-meta").textContent).toBe("Komi 0.5 · 3 handicap · Tromp-Taylor rules");
  expect(screen.getByTestId("game-meta").getAttribute("title")).toBeNull();
});

test("the player rows show the match rate once the strength estimate has one", () => {
  const board: BoardView = {
    width: 19, height: 19, stones: [], last_move: null, to_play: "Black", move_number: 0, captured_black: 0, captured_white: 0,
  };
  const { rerender } = render(
    <InfoPanel board={board} black={{ name: "甲", rank: null, match: 0.736 }} white={{ name: "乙", rank: null, match: null }} points={[]} currentIndex={0} labels={zh.info} />,
  );
  expect(screen.getByTestId("player-match-black").textContent).toBe("吻合 74%");
  expect(screen.getByTestId("player-match-black").getAttribute("title")).toBe(zh.info.matchHint);
  expect(screen.queryByTestId("player-match-white")).toBeNull();

  rerender(<InfoPanel board={board} black={{ name: "甲", rank: null }} white={{ name: "乙", rank: null }} points={[]} currentIndex={0} labels={zh.info} />);
  expect(screen.queryByTestId("player-match-black")).toBeNull();
});
