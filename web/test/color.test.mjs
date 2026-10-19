// The melanin rule (ADR-0005 section 5) against the Python reference
// (tools/melanin_reference.py, its cases in fixtures/melanin.json): every support point
// gives its table colour back, and between the points every colour is the reference's.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { MELANIN_TO_SRGB } from "../dist/generated/body.js";
import { gridColor, hex, linear, rampColor } from "../dist/render/color.js";

const read = (name) => JSON.parse(readFileSync(new URL(`./fixtures/${name}`, import.meta.url), "utf8"));
const cases = read("melanin.json");
// Every byte through the transfer curve and back, and inputs a billionth beside each support
// point; the exact half ties the reference found stay out: which way they round depends on
// the order of the float operations, which the rule does not fix.
const boundaries = read("melanin_boundaries.json");

function colour(table, input) {
  return table === "iris"
    ? rampColor(MELANIN_TO_SRGB.iris, input[0])
    : gridColor(MELANIN_TO_SRGB[table], input[0], input[1]);
}

test("each case of the reference has its colour", () => {
  assert.ok(cases.length > 100, `${cases.length} cases`);
  for (const { table, input, hex } of cases) {
    assert.equal(colour(table, input), hex, `${table} ${JSON.stringify(input)}`);
  }
});

test("every support point gives its table colour back", () => {
  for (const table of ["skin", "hair"]) {
    const grid = MELANIN_TO_SRGB[table];
    grid.eumelanin.forEach((e, i) => {
      grid.pheomelanin.forEach((p, j) => {
        assert.equal(gridColor(grid, e, p), grid.srgb[i][j], `${table} ${e} ${p}`);
      });
    });
  }
  const iris = MELANIN_TO_SRGB.iris;
  iris.melanin.forEach((m, i) => assert.equal(rampColor(iris, m), iris.srgb[i], `iris ${m}`));
});

test("a value outside [0, 1] takes the colour of its bound", () => {
  const skin = MELANIN_TO_SRGB.skin;
  assert.equal(gridColor(skin, -0.5, 1.5), gridColor(skin, 0, 1));
  assert.equal(rampColor(MELANIN_TO_SRGB.iris, 1.1), MELANIN_TO_SRGB.iris.srgb[4]);
});

test("NaN is refused, an infinity is clamped", () => {
  const skin = MELANIN_TO_SRGB.skin;
  assert.throws(() => gridColor(skin, Number.NaN, 0.4), RangeError);
  assert.throws(() => rampColor(MELANIN_TO_SRGB.iris, Number.NaN), RangeError);
  assert.equal(gridColor(skin, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY), skin.srgb[4][0]);
});

test("every byte goes through the transfer curve and back", () => {
  assert.equal(boundaries.bytes.length, 256);
  for (const { byte, linear: expected, back } of boundaries.bytes) {
    const text = byte.toString(16).padStart(2, "0");
    const [r, g, b] = linear(`#${text}${text}${text}`);
    assert.ok(Math.abs(r - expected) <= 1e-15 && r === g && g === b, `byte ${byte}: ${r} against ${expected}`);
    assert.equal(back, byte);
    assert.equal(hex([r, g, b]), `#${text}${text}${text}`);
  }
});

test("beside a support point the colour is the point's", () => {
  assert.equal(boundaries.beside.length, 50);
  for (const { table, input, hex: expected } of boundaries.beside) {
    assert.equal(colour(table, input), expected, `${table} ${JSON.stringify(input)}`);
  }
});
