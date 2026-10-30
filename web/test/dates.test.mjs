// A date of the core as the viewer writes it (ADR-0002: the display counts from 1).
import { test } from "node:test";
import assert from "node:assert/strict";

import { bornText, captionYear, dateText } from "../dist/dates.js";

test("a date reads its year, month and day, counted from one", () => {
  const date = (year, month, day_of_month) => ({ year, day_of_year: 0, month, day_of_month, week: 0, weekday: 0, season: 0, second_of_day: 0 });
  assert.equal(dateText(date(29, 0, 0)), "Jahr 29, 1. Monat, 1. Tag");
  assert.equal(dateText(date(15, 5, 23)), "Jahr 15, 6. Monat, 24. Tag");
});

// A date before the founding (a couple of the pool paired before it, in the comparison) names
// its year like "21 v. Chr.": year -n is "Jahr n vor der Gründung", the same year as "im n.
// Jahr vor der Gründung" of a birth (Grok's cross-check), year -1 "Jahr 1 vor der Gründung".
test("a date before the founding names its year so", () => {
  const date = (year, month, day_of_month) => ({ year, day_of_year: 0, month, day_of_month, week: 0, weekday: 0, season: 0, second_of_day: 0 });
  assert.equal(dateText(date(-3, 12, 27)), "Jahr 3 vor der Gründung, 13. Monat, 28. Tag");
  assert.equal(dateText(date(-1, 0, 0)), "Jahr 1 vor der Gründung, 1. Monat, 1. Tag");
  assert.equal(dateText(date(0, 0, 0)), "Jahr 0, 1. Monat, 1. Tag");
});

// The second playtest: an arrival from the pool, born before the founding, read "geboren im
// Jahr -21" and "mit 2 (-19)" under a portrait. A year before the founding is named so; from
// year 0 on, the year as before.
test("a year before the founding is named so", () => {
  assert.equal(bornText(22), "geboren im Jahr 22");
  assert.equal(bornText(0), "geboren im Jahr 0");
  // Year -21 begins 21 years before the founding and ends 20 years before it: a birth in it
  // is in the 21st year before the founding (Grok's wording).
  assert.equal(bornText(-21), "geboren im 21. Jahr vor der Gründung");
  assert.equal(bornText(-1), "geboren im Jahr vor der Gründung");
  assert.equal(captionYear(4), "4");
  assert.equal(captionYear(0), "0");
  assert.equal(captionYear(-19), "vor der Gründung");
});
