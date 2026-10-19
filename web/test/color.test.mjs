// The melanin rule (ADR-0005 section 5) against the Python reference
// (tools/melanin_reference.py, its cases in fixtures/melanin.json): every support point
// gives its table colour back, and between the points every colour is the reference's.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { MELANIN_TO_SRGB } from "../dist/generated/body.js";
import { gridColor, rampColor } from "../dist/render/color.js";

const cases = JSON.parse(readFileSync(new URL("./fixtures/melanin.json", import.meta.url), "utf8"));

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
