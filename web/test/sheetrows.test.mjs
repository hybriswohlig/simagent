// The rows of the sheet of portraits (ADR-0021 G10; texts Grok's round 38, T3 to T5): each
// founder of the answer in its order, "{variant}, weiblich" or "männlich", a portrait per
// age with the title of row and age, "2 Jahre". The answer is shortened by hand; every
// expected text by hand from Grok's wording.
import { test } from "node:test";
import assert from "node:assert/strict";

import { heightText, sheetRows, yearsText } from "../dist/sheetrows.js";

const body = (height_cm) => ({ height_cm });
const shown = (age_years, height_cm) => ({ at: { t: 0, age_years, calendar_year: 0 }, body: body(height_cm) });

const ANSWER = {
  format: "simagent.view.sheet",
  version: 1,
  contract_version: "0.1.0",
  rows: [
    { variant: "early", sex: "female", at_ages: [shown(2, 86), shown(60, 162)] },
    { variant: "base", sex: "male", at_ages: [shown(2, 88)] },
    { variant: "late", sex: "female", at_ages: [] },
    { variant: "red", sex: "male", at_ages: [] },
    { variant: "light_iris", sex: "female", at_ages: [] },
  ],
};

test("an age is its years", () => {
  assert.equal(yearsText(2), "2 Jahre");
  assert.equal(yearsText(60), "60 Jahre");
});

// K10 of the third playtest (Grok's decision): under each portrait its height from the
// answer in whole centimetres, from a half up, so that maturity shows at 12 and 16 years.
test("a height is its whole centimetres", () => {
  assert.equal(heightText(162.017), "162 cm");
  assert.equal(heightText(154.667), "155 cm");
  assert.equal(heightText(149.5), "150 cm");
  assert.equal(heightText(149.49), "149 cm");
  assert.equal(heightText(86), "86 cm");
  // The contract's ends, and a value just below a half (Astra's review).
  assert.equal(heightText(40), "40 cm");
  assert.equal(heightText(230), "230 cm");
  assert.equal(heightText(149.49999999999997), "149 cm");
});

test("each founder a row in the answer's order, with the names of the variants", () => {
  const rows = sheetRows(ANSWER);
  assert.deepEqual(
    rows.map((row) => row.label),
    [
      "Frühe Reife, weiblich",
      "Gewöhnliche Reife, männlich",
      "Späte Reife, weiblich",
      "Rotes Haar, männlich",
      "Helle Iris, weiblich",
    ],
  );
  // The portraits of a row take its sex, for the hair's length.
  assert.deepEqual(
    rows.map((row) => row.sex),
    ["female", "male", "female", "male", "female"],
  );
  assert.deepEqual(rows[0].cells, [
    { age: 2, title: "Frühe Reife, weiblich, 2 Jahre", height: "86 cm", body: body(86) },
    { age: 60, title: "Frühe Reife, weiblich, 60 Jahre", height: "162 cm", body: body(162) },
  ]);
  assert.deepEqual(rows[1].cells, [{ age: 2, title: "Gewöhnliche Reife, männlich, 2 Jahre", height: "88 cm", body: body(88) }]);
});
