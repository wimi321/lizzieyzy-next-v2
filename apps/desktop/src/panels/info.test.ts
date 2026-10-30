import { describe, expect, test } from "vitest";
import type { GraphPointView, MoveView } from "@/api/backend";
import { lastMoveChange, leadOf, signed } from "./info";

function point(move: number, winrate: number | null, score = 0): GraphPointView {
  return { node_id: move, move_number: move, summary: winrate === null ? null : { winrate, score_lead: score, visits: 500 } };
}

const blackMove: MoveView = { type: "Play", data: { color: "Black", point: { x: 3, y: 3 } } };
const whitePass: MoveView = { type: "Pass", data: { color: "White" } };

describe("last move change", () => {
  test("a Black move that raises Black's winrate is a gain for Black", () => {
    const change = lastMoveChange([point(0, 0.45, -1), point(1, 0.52, 0.5)], 1, blackMove);
    expect(change?.moveNumber).toBe(1);
    expect(change?.winrate).toBeCloseTo(7);
    expect(change?.score).toBeCloseTo(1.5);
  });

  test("a White move is measured from White's side", () => {
    const change = lastMoveChange([point(0, 0.45, -1), point(1, 0.52, 0.5)], 1, whitePass);
    expect(change?.winrate).toBeCloseTo(-7);
    expect(change?.score).toBeCloseTo(-1.5);
  });

  test("needs both evaluations and a move", () => {
    expect(lastMoveChange([point(0, null), point(1, 0.5)], 1, blackMove)).toBeNull();
    expect(lastMoveChange([point(0, 0.5), point(1, null)], 1, blackMove)).toBeNull();
    expect(lastMoveChange([point(0, 0.5)], 0, null)).toBeNull();
  });
});

describe("lead", () => {
  test("Black's score lead decides the side; rounding decides an even game", () => {
    expect(leadOf({ winrate: 0.6, score_lead: 2.34, visits: 1 })).toEqual({ side: "Black", points: 2.3 });
    expect(leadOf({ winrate: 0.4, score_lead: -0.66, visits: 1 })).toEqual({ side: "White", points: 0.7 });
    expect(leadOf({ winrate: 0.5, score_lead: 0.04, visits: 1 })).toEqual({ side: "Even" });
  });

  test("signed numbers use a real minus sign and no negative zero", () => {
    expect(signed(0.34)).toBe("+0.3");
    expect(signed(-1.26)).toBe("−1.3");
    expect(signed(-0.04)).toBe("0.0");
  });
});
