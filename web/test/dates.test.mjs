// A date of the core as the viewer writes it (ADR-0002: the display counts from 1).
import { test } from "node:test";
import assert from "node:assert/strict";

import { dateText } from "../dist/dates.js";

test("a date reads its year, month and day, counted from one", () => {
  const date = (year, month, day_of_month) => ({ year, day_of_year: 0, month, day_of_month, week: 0, weekday: 0, season: 0, second_of_day: 0 });
  assert.equal(dateText(date(29, 0, 0)), "Jahr 29, 1. Monat, 1. Tag");
  assert.equal(dateText(date(15, 5, 23)), "Jahr 15, 6. Monat, 24. Tag");
  assert.equal(dateText(date(-3, 12, 27)), "Jahr -3, 13. Monat, 28. Tag");
});
