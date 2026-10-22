// The geometry of a census curve: a fixed box 460 by 200, the plot from x 48 to 448 and from
// y 12 to 172; x linear in the year, y linear between the axis ends; the axis step the
// smallest of 1, 2 and 5 times a power of ten (not below the shown decimals) that needs at
// most five steps. Every expected coordinate is worked out by hand from those rules.
import { test } from "node:test";
import assert from "node:assert/strict";

import { chart } from "../dist/render/chart.js";

/** Equal up to binary64 noise of the arithmetic. */
function close(actual, expected) {
  assert.ok(Math.abs(actual - expected) < 1e-9, `${actual} is not ${expected}`);
}

function closePoints(actual, expected) {
  assert.equal(actual.length, expected.length);
  actual.forEach((point, i) => {
    close(point.x, expected[i][0]);
    close(point.y, expected[i][1]);
  });
}

test("a line breaks where a year has no point, on an axis from zero", () => {
  // max 10: step 1 needs 10 steps, step 2 needs 5. y(v) = 12 + 160 (1 - v/10).
  const drawn = chart([0, 1, 2, 3, 4], [[0, 10, null, 5, 10], [2, 2, 2, 2, 2]], true, 0);
  assert.equal(drawn.width, 460);
  assert.equal(drawn.height, 200);
  assert.deepEqual(drawn.yTicks.map((tick) => tick.value), [0, 2, 4, 6, 8, 10]);
  drawn.yTicks.forEach((tick, i) => close(tick.at, [172, 140, 108, 76, 44, 12][i]));
  assert.deepEqual(drawn.xTicks.map((tick) => tick.value), [0, 1, 2, 3, 4]);
  drawn.xTicks.forEach((tick, i) => close(tick.at, [48, 148, 248, 348, 448][i]));
  assert.equal(drawn.segments.length, 2);
  assert.equal(drawn.segments[0].length, 2);
  closePoints(drawn.segments[0][0], [[48, 172], [148, 12]]);
  closePoints(drawn.segments[0][1], [[348, 92], [448, 12]]);
  assert.equal(drawn.segments[1].length, 1);
  closePoints(drawn.segments[1][0], [[48, 140], [148, 140], [248, 140], [348, 140], [448, 140]]);
  assert.deepEqual(drawn.dots, [[], []]);
});

test("a lone value is a dot, not a line", () => {
  // years 0 to 2: year 1 is the middle, 248; max 3 with step 1: y(3) = 12.
  const drawn = chart([0, 1, 2], [[null, 3, null]], true, 0);
  assert.deepEqual(drawn.segments, [[]]);
  assert.equal(drawn.dots[0].length, 1);
  closePoints(drawn.dots[0], [[248, 12]]);
});

test("heights get an axis around their values", () => {
  // 165.3 and 170.2 with one decimal: step 1 needs 171 - 165 = 6 steps, step 2 needs
  // 86 - 82 = 4, so 164 to 172. y(165.3) = 12 + 160 (1 - 1.3/8) = 146,
  // y(170.2) = 12 + 160 (1 - 6.2/8) = 48.
  const drawn = chart([0, 1], [[165.3, 170.2]], false, 1);
  assert.deepEqual(drawn.yTicks.map((tick) => tick.value), [164, 166, 168, 170, 172]);
  closePoints(drawn.segments[0][0], [[48, 146], [448, 48]]);
});

test("a small percent gets a step of hundredths", () => {
  // 0.03 with two decimals: step 0.01 needs 3 steps; 0 to 0.03.
  const drawn = chart([0, 1], [[0, 0.03]], true, 2);
  assert.equal(drawn.yTicks.length, 4);
  drawn.yTicks.forEach((tick, i) => close(tick.value, [0, 0.01, 0.02, 0.03][i]));
  closePoints(drawn.segments[0][0], [[48, 172], [448, 12]]);
});

test("one year sits in the middle", () => {
  const drawn = chart([7], [[4]], true, 0);
  assert.deepEqual(drawn.xTicks.map((tick) => tick.value), [7]);
  close(drawn.xTicks[0].at, 248);
  closePoints(drawn.dots[0], [[248, 12]]);
});

test("the year ticks skip on a long run", () => {
  // years -35 to 30, 65 years: steps 1, 2, 5 and 10 need more than five, 20 needs 3.25.
  // x(-20) = 48 + 400 (15/65), x(0) = 48 + 400 (35/65), x(20) = 48 + 400 (55/65).
  const years = Array.from({ length: 66 }, (_, i) => i - 35);
  const drawn = chart(years, [years.map(() => 1)], true, 0);
  assert.deepEqual(drawn.xTicks.map((tick) => tick.value), [-20, 0, 20]);
  drawn.xTicks.forEach((tick, i) => close(tick.at, 48 + (400 * [15, 35, 55][i]) / 65));
});

test("no value draws nothing on an axis from 0 to 1", () => {
  const drawn = chart([0, 1], [[null, null]], true, 0);
  assert.deepEqual(drawn.yTicks.map((tick) => tick.value), [0, 1]);
  assert.deepEqual(drawn.segments, [[]]);
  assert.deepEqual(drawn.dots, [[]]);
});

test("equal values get one step above them", () => {
  // 5 and 5, whole numbers, not from zero: step 1 needs 0 steps, at least one: 5 to 6.
  const drawn = chart([0, 1], [[5, 5]], false, 0);
  assert.deepEqual(drawn.yTicks.map((tick) => tick.value), [5, 6]);
  closePoints(drawn.segments[0][0], [[48, 172], [448, 172]]);
});

test("no year draws nothing", () => {
  const drawn = chart([], [[]], true, 0);
  assert.deepEqual(drawn.xTicks, []);
  assert.deepEqual(drawn.segments, [[]]);
  assert.deepEqual(drawn.dots, [[]]);
});
