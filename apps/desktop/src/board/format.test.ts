import { describe, expect, test } from "vitest";
import type { CandidateView } from "@/api/backend";
import {
  candidateTier,
  columnLabel,
  forMover,
  formatScore,
  formatVisits,
  formatWinrate,
  rowLabel,
  winrateLoss,
} from "./format";

function candidate(winrate: number, scoreLead = 0): CandidateView {
  return { coordinate: "D4", point: { x: 3, y: 15 }, visits: 100, winrate, score_lead: scoreLead, prior: 0, order: 0, pv: [] };
}

describe("color tiers", () => {
  test("the engine's first choice is tier 0 whatever its numbers", () => {
    expect(candidateTier(0, true)).toBe(0);
    expect(candidateTier(30, true)).toBe(0);
  });

  test.each([
    [0, 1],
    [1, 1],
    [1.01, 2],
    [3, 2],
    [5.9, 3],
    [6, 3],
    [11.9, 4],
    [12.5, 5],
    [60, 5],
  ])("losing %f points is tier %i", (loss, tier) => {
    expect(candidateTier(loss, false)).toBe(tier);
  });

  test("losses are measured for the side to move", () => {
    const best = candidate(0.4);
    // White to move: a lower Black winrate is better for White.
    expect(winrateLoss(candidate(0.45), best, "White")).toBeCloseTo(5);
    expect(winrateLoss(candidate(0.45), candidate(0.5), "Black")).toBeCloseTo(5);
    expect(winrateLoss(candidate(0.6), candidate(0.5), "Black")).toBe(0);
  });

  test("a loss that reads 6.0 on the labels stays in the 6-point tier", () => {
    expect(candidateTier(winrateLoss(candidate(0.502), candidate(0.562), "Black"), false)).toBe(3);
  });
});

describe("labels", () => {
  test("values are shown for the side to move", () => {
    expect(forMover(0.7, "Black", "winrate")).toBeCloseTo(0.7);
    expect(forMover(0.7, "White", "winrate")).toBeCloseTo(0.3);
    expect(forMover(2.5, "White", "score")).toBe(-2.5);
  });

  test.each([
    [0.5, "50.0"],
    [0.4567, "45.7"],
    [1, "100.0"],
    [1.2, "100.0"],
    [-0.1, "0.0"],
  ])("winrate %f is %s", (value, text) => {
    expect(formatWinrate(value)).toBe(text);
  });

  test.each([
    [0, "0"],
    [999, "999"],
    [1000, "1k"],
    [1234, "1.2k"],
    [12_345, "12k"],
    [999_499, "999k"],
    [1_250_000, "1.3m"],
    [12_000_000, "12m"],
  ])("%i visits read %s", (visits, text) => {
    expect(formatVisits(visits)).toBe(text);
  });

  test.each([
    [1.25, "+1.3"],
    [-0.44, "−0.4"],
    [0.04, "0.0"],
    [-0.04, "0.0"],
    [12, "+12.0"],
  ])("score %f reads %s", (value, text) => {
    expect(formatScore(value)).toBe(text);
  });

  test("coordinates follow GTP: no I, rows counted from the bottom", () => {
    expect(columnLabel(7, 19)).toBe("H");
    expect(columnLabel(8, 19)).toBe("J");
    expect(columnLabel(18, 19)).toBe("T");
    expect(rowLabel(0, 19)).toBe("19");
    expect(rowLabel(18, 19)).toBe("1");
    expect(rowLabel(0, 13)).toBe("13");
  });
});
