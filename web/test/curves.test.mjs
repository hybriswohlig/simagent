// The curves of a run from its census (ADR-0021, addendum 2026-10-21; Grok R1 to R4, S1):
// seven pictures in their order, the columns read by name, the lines named as on the card,
// no point where nobody was counted, heights in cm with one decimal, the newborns' inbreeding
// in percent with two, and the card's sentence above the affected and the deaths when the
// switch is off at the end of the last year. Every expected value is written by hand.
import { test } from "node:test";
import assert from "node:assert/strict";

import { UnknownAnswer } from "../dist/api.js";
import { curves } from "../dist/curves.js";

const DISEASES = [
  "core:disease_ar",
  "core:disease_ad_late",
  "core:disease_ad_partial",
  "core:disease_xr",
  "core:disease_mt",
  "core:disease_liability",
  "core:disease_new",
];
const LETHAL = DISEASES.filter((code) => code !== "core:disease_mt");

const COLUMNS = [
  "year", "living", "lived_secs", "body_evaluated", "curve_clamped",
  "births", "arrivals", "emigrations",
  "deaths.core:senescence", "deaths.core:accident", ...LETHAL.map((code) => `deaths.${code}`),
  "diseases_on", ...DISEASES.map((code) => `affected.${code}`),
  "adults_female", "adult_height_mm_female", "adults_male", "adult_height_mm_male",
  "age25_female", "age25_height_mm_female", "age25_male", "age25_height_mm_male",
  "inbreeding_e4",
];

/** A row from its named cells; every other column 0. */
function row(cells) {
  return COLUMNS.map((column) => cells[column] ?? 0);
}

const YEAR_0 = row({
  year: 0, living: 51, births: 9, arrivals: 1, "deaths.core:senescence": 1,
  "deaths.core:disease_liability": 2, diseases_on: 1, "affected.core:disease_ar": 1,
  "affected.core:disease_mt": 1, adults_female: 21, adult_height_mm_female: 1660,
  adults_male: 19, adult_height_mm_male: 1770, age25_female: 1, age25_height_mm_female: 1674,
});
const YEAR_1 = row({
  year: 1, living: 55, emigrations: 2, diseases_on: 1, "affected.core:disease_ar": 2,
  adults_female: 21, adult_height_mm_female: 1661, adults_male: 20, adult_height_mm_male: 1771,
  age25_male: 2, age25_height_mm_male: 1802,
});
const YEAR_2 = row({
  year: 2, living: 56, births: 2, "deaths.core:accident": 1, adults_female: 22,
  adult_height_mm_female: 1674, adults_male: 20, adult_height_mm_male: 1772, age25_female: 2,
  age25_height_mm_female: 1655, age25_male: 1, age25_height_mm_male: 1790, inbreeding_e4: 78,
});

function answer(rows) {
  return { format: "simagent.view.census", version: 1, columns: COLUMNS, rows };
}

const OFF = "Erbkrankheiten sind in diesem Lauf ausgeschaltet.";

test("seven pictures in their order", () => {
  const shown = curves(answer([YEAR_0, YEAR_1, YEAR_2]));
  assert.deepEqual(shown.years, [0, 1, 2]);
  assert.deepEqual(
    shown.curves.map((curve) => curve.title),
    [
      "Lebende",
      "Geburten, Zuzüge und Wegzüge",
      "Tode",
      "Betroffene je Erbkrankheit",
      "Mittlere Größe der Erwachsenen",
      "Mittlere Größe mit 25",
      "Mittlere Inzucht der Neugeborenen",
    ],
  );
  assert.deepEqual(shown.curves.map((curve) => curve.fromZero), [true, true, true, true, false, false, true]);
  assert.deepEqual(shown.curves.map((curve) => curve.decimals), [0, 0, 0, 0, 1, 1, 2]);
});

test("the population by name", () => {
  const [living, moves] = curves(answer([YEAR_0, YEAR_1, YEAR_2])).curves;
  assert.deepEqual(living.lines, [{ name: "Lebende", values: [51, 55, 56], last: "56" }]);
  assert.deepEqual(moves.lines, [
    { name: "Geburten", values: [9, 0, 2], last: "2" },
    { name: "Zuzüge", values: [1, 0, 0], last: "0" },
    { name: "Wegzüge", values: [0, 2, 0], last: "0" },
  ]);
});

test("the deaths by cause and the affected by disease, named as on the card", () => {
  const [, , deaths, affected] = curves(answer([YEAR_0, YEAR_1, YEAR_2])).curves;
  assert.deepEqual(
    deaths.lines.map((line) => line.name),
    ["Alter", "Unfall", "Zäher Atem", "Später Verfall", "Ungleicher Ausbruch", "Schwere Blutung", "Geteilte Schwere", "core:disease_new"],
  );
  assert.deepEqual(deaths.lines[0].values, [1, 0, 0]);
  assert.deepEqual(deaths.lines[1].values, [0, 0, 1]);
  assert.deepEqual(deaths.lines[6].values, [2, 0, 0]);
  assert.deepEqual(
    affected.lines.map((line) => line.name),
    ["Zäher Atem", "Später Verfall", "Ungleicher Ausbruch", "Schwere Blutung", "Mütterliches Augenlicht", "Geteilte Schwere", "core:disease_new"],
  );
  assert.deepEqual(affected.lines[0].values, [1, 2, 0]);
  assert.deepEqual(affected.lines[4].values, [1, 0, 0]);
});

test("the heights in cm, without a point where nobody was counted", () => {
  const [, , , , adults, age25] = curves(answer([YEAR_0, YEAR_1, YEAR_2])).curves;
  assert.deepEqual(adults.lines, [
    { name: "Frauen", values: [166, 166.1, 167.4], last: "167,4" },
    { name: "Männer", values: [177, 177.1, 177.2], last: "177,2" },
  ]);
  assert.deepEqual(age25.lines, [
    { name: "Frauen", values: [167.4, null, 165.5], last: "165,5" },
    { name: "Männer", values: [null, 180.2, 179], last: "179,0" },
  ]);
  assert.equal(adults.label(166), "166,0");
  assert.equal(adults.tick(164), "164");
});

test("the newborns' inbreeding in percent, without a point in a year without births", () => {
  const inbreeding = curves(answer([YEAR_0, YEAR_1, YEAR_2])).curves[6];
  assert.deepEqual(inbreeding.lines, [
    { name: "Mittlere Inzucht der Neugeborenen", values: [0, null, 0.78], last: "0,78 Prozent" },
  ]);
  assert.equal(inbreeding.label(0), "0,00 Prozent");
  assert.equal(inbreeding.tick(0.2), "0,2");
});

test("a last year without a point names nothing", () => {
  const inbreeding = curves(answer([YEAR_0, YEAR_1])).curves[6];
  assert.deepEqual(inbreeding.lines[0].values, [0, null]);
  assert.equal(inbreeding.lines[0].last, null);
});

test("the sentence stands above the affected and the deaths when the last year ends off", () => {
  const ended = curves(answer([YEAR_0, YEAR_1, YEAR_2]));
  assert.equal(ended.off, true);
  assert.deepEqual(ended.curves.map((curve) => curve.note), [null, null, OFF, OFF, null, null, null]);
  const back = curves(answer([YEAR_2, YEAR_1].map((cells, i) => [i, ...cells.slice(1)])));
  assert.equal(back.off, false);
  assert.deepEqual(back.curves.map((curve) => curve.note), [null, null, null, null, null, null, null]);
});

test("no full year yet gives empty lines", () => {
  const shown = curves(answer([]));
  assert.deepEqual(shown.years, []);
  assert.equal(shown.off, false);
  assert.deepEqual(shown.curves[0].lines, [{ name: "Lebende", values: [], last: null }]);
});

test("a census without a column of the curves or with a short row is unknown", () => {
  const without = { ...answer([YEAR_0]), columns: COLUMNS.filter((column) => column !== "inbreeding_e4") };
  assert.throws(() => curves({ ...without, rows: [YEAR_0.slice(0, -1)] }), UnknownAnswer);
  assert.throws(() => curves(answer([YEAR_0.slice(0, -1)])), UnknownAnswer);
});
