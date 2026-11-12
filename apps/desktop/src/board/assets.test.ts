import { expect, test } from "vitest";
import { devicePixels } from "./assets";

test("the browser's count of device pixels wins when it agrees with the scale", () => {
  // 834 CSS px at 125% is 1042.5 device px; the browser laid it out on 1042.
  expect(devicePixels(1042, 834, 1.25)).toBe(1042);
  expect(devicePixels(1043, 834, 1.25)).toBe(1043);
  expect(devicePixels(834, 834, 1)).toBe(834);
});

test("without a count that fits, the length is scaled and rounded", () => {
  // WebKit reports nothing.
  expect(devicePixels(undefined, 834, 2)).toBe(1668);
  expect(devicePixels(undefined, 833.5, 2)).toBe(1667);
  // An emulated scale reports CSS pixels.
  expect(devicePixels(834, 834, 2)).toBe(1668);
  expect(devicePixels(834, 834, 1.25)).toBe(1043);
});
