import { describe, expect, test } from "vitest";
import { backingSize, computeGeometry, pixelToPoint, pointIndex, pointToPixel, starPoints } from "./geometry";

describe("pixel and point conversion", () => {
  test.each([
    [19, 19, 800, 800, 1, true],
    [19, 19, 640, 900, 2, false],
    [13, 13, 500, 420, 1.5, true],
    [9, 9, 300, 300, 3, false],
    [9, 13, 600, 400, 2, true],
  ])("%ix%i in %ix%i CSS px at DPR %f: every point round-trips", (columns, rows, width, height, dpr, coordinates) => {
    const geometry = computeGeometry(columns, rows, width, height, dpr, coordinates);
    for (let y = 0; y < rows; y++) {
      for (let x = 0; x < columns; x++) {
        const pixel = pointToPixel(geometry, { x, y });
        expect(pixelToPoint(geometry, pixel.x, pixel.y)).toEqual({ x, y });
        // Anywhere inside the half-cell square still hits the same point.
        const nudge = geometry.cell * 0.45;
        expect(pixelToPoint(geometry, pixel.x + nudge, pixel.y - nudge)).toEqual({ x, y });
      }
    }
    const { width: deviceWidth, height: deviceHeight } = backingSize(geometry);
    expect(deviceWidth).toBe(Math.round(width * dpr));
    expect(deviceHeight).toBe(Math.round(height * dpr));
  });

  test("the board is centered and fits the shorter side, margins included", () => {
    const geometry = computeGeometry(19, 19, 1000, 700, 2, true);
    const span = geometry.cell * 18 + geometry.margin * 2;
    expect(span).toBeCloseTo(700);
    expect(geometry.originX - geometry.margin).toBeCloseTo((1000 - span) / 2);
    expect(geometry.originY - geometry.margin).toBeCloseTo(0);
  });

  test("a rectangular board keeps width and height apart", () => {
    const geometry = computeGeometry(9, 13, 900, 1300, 1, false);
    expect(pixelToPoint(geometry, geometry.originX + 8 * geometry.cell, geometry.originY + 12 * geometry.cell)).toEqual({
      x: 8,
      y: 12,
    });
    expect(pixelToPoint(geometry, geometry.originX + 9 * geometry.cell, geometry.originY)).toBeNull();
    expect(pointIndex(9, { x: 8, y: 12 })).toBe(116);
  });

  test("clicks outside the grid or before layout hit nothing", () => {
    const geometry = computeGeometry(19, 19, 800, 800, 1, true);
    expect(pixelToPoint(geometry, 1, 1)).toBeNull();
    expect(pixelToPoint(geometry, geometry.originX - geometry.cell * 0.6, geometry.originY)).toBeNull();
    expect(pixelToPoint(computeGeometry(19, 19, 0, 0, 1, true), 0, 0)).toBeNull();
  });
});

describe("star points", () => {
  test.each([
    [19, 19, 9],
    [13, 13, 5],
    [9, 9, 5],
    [9, 13, 5],
    [10, 10, 4],
    [5, 5, 0],
  ])("%ix%i has %i", (columns, rows, count) => {
    expect(starPoints(columns, rows)).toHaveLength(count);
  });

  test("19x19 uses the 4-4 lines and the center", () => {
    expect(starPoints(19, 19)).toContainEqual({ x: 3, y: 3 });
    expect(starPoints(19, 19)).toContainEqual({ x: 9, y: 9 });
    expect(starPoints(19, 19)).toContainEqual({ x: 15, y: 9 });
    expect(starPoints(9, 9)).toEqual([
      { x: 2, y: 2 },
      { x: 6, y: 2 },
      { x: 4, y: 4 },
      { x: 2, y: 6 },
      { x: 6, y: 6 },
    ]);
  });
});
