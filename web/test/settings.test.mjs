// Own values when a town is founded (ADR-0021, addendum of 2026-11-05): the fields start at
// the base game's values the server names, the founding sends only the values that differ
// from them, each field says what it does and what the base game gives with the server's
// numbers, and a town with own values is marked with them. Every expected value by hand.
import { test } from "node:test";
import assert from "node:assert/strict";

import { changedValues, fieldsOf, foundBody, hintOf, labelOf, markOf } from "../dist/settings.js";

// As the run list of the server names them, version 2.
const FOUNDING = [
  { key: "core:market_age", base: 16, outcome: null },
  { key: "core:market_search_center_female", base: 25.23, outcome: 23.5 },
  { key: "core:pairing_age_gap_preferred", base: 4.69, outcome: 3 },
];

test("the fields start at the base game's values", () => {
  assert.deepEqual(fieldsOf(FOUNDING), {
    "core:market_age": "16",
    "core:market_search_center_female": "25.23",
    "core:pairing_age_gap_preferred": "4.69",
  });
});

test("only the values that differ from the base game's are sent", () => {
  const fields = fieldsOf(FOUNDING);
  assert.deepEqual(changedValues(FOUNDING, fields), {});
  assert.deepEqual(changedValues(FOUNDING, { ...fields, "core:market_age": "18" }), { "core:market_age": 18 });
  assert.deepEqual(
    changedValues(FOUNDING, { ...fields, "core:market_age": "16.0", "core:pairing_age_gap_preferred": "2.5" }),
    { "core:pairing_age_gap_preferred": 2.5 },
  );
  assert.deepEqual(
    changedValues(FOUNDING, {
      "core:market_age": "17",
      "core:market_search_center_female": "27",
      "core:pairing_age_gap_preferred": "0",
    }),
    { "core:market_age": 17, "core:market_search_center_female": 27, "core:pairing_age_gap_preferred": 0 },
  );
  // A field without a number sends nothing: the founding waits for it.
  for (const text of ["", " ", "x", "1e400", "-1e400"]) {
    assert.equal(changedValues(FOUNDING, { ...fields, "core:market_age": text }), null, text);
  }
  const { "core:market_age": _, ...without } = fields;
  assert.equal(changedValues(FOUNDING, without), null);
});

test("the body of a founding names own values only when there are some", () => {
  assert.equal(JSON.stringify(foundBody("stadt", "neu", {})), '{"template":"stadt","run":"neu"}');
  assert.equal(
    JSON.stringify(foundBody("stadt", "eigen", { "core:market_age": 18 })),
    '{"template":"stadt","run":"eigen","settings":{"core:market_age":18}}',
  );
});

test("each field says what it does and what the base game gives", () => {
  assert.equal(labelOf("core:market_age"), "Partnerschaften ab (Jahre)");
  assert.equal(labelOf("core:market_search_center_female"), "Frauen suchen am häufigsten mit (Jahre)");
  assert.equal(labelOf("core:pairing_age_gap_preferred"), "Bevorzugter Altersabstand, Mann älter (Jahre)");
  assert.equal(labelOf("female_ramp"), "female_ramp");
  assert.equal(hintOf(FOUNDING[0]), "Wer jünger ist, geht keine Partnerschaft ein. Im Grundspiel: 16.");
  assert.equal(
    hintOf(FOUNDING[1]),
    "Um dieses Alter suchen Frauen am eifrigsten einen Partner. Im Grundspiel (25,23) hat die Hälfte der Frauen mit etwa 23,5 Jahren schon ihre erste Partnerschaft.",
  );
  assert.equal(
    hintOf(FOUNDING[2]),
    "So viele Jahre älter als die Frau ist der Mann am liebsten. Im Grundspiel (4,69) ist er im Mittel etwa 3 Jahre älter.",
  );
  // Without the server's number of what it gives, the base game's value alone.
  assert.equal(
    hintOf({ key: "core:pairing_age_gap_preferred", base: 4.69, outcome: null }),
    "So viele Jahre älter als die Frau ist der Mann am liebsten. Im Grundspiel: 4,69.",
  );
  assert.equal(hintOf({ key: "female_ramp", base: 4, outcome: null }), "Im Grundspiel: 4.");
});

test("a town with own values is marked with them", () => {
  assert.equal(markOf([]), null);
  assert.equal(markOf([{ key: "core:market_age", value: 18 }]), "Partnerschaften ab 18 Jahren");
  assert.equal(
    markOf([
      { key: "core:market_search_center_female", value: 27.5 },
      { key: "core:pairing_age_gap_preferred", value: 2 },
    ]),
    "Frauen suchen am häufigsten mit 27,5 Jahren; Altersabstand 2 Jahre, Mann älter",
  );
  assert.equal(markOf([{ key: "female_ramp", value: 4.25 }]), "female_ramp: 4,25");
});
