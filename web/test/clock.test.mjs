// The clock of a run page asks for one step at a time; playing asks for the same step after
// each answer until a pause or a refusal (ADR-0021 section 2). Pausing is no input.
import { test } from "node:test";
import assert from "node:assert/strict";

import { answered, pause, play, press, refused, standing } from "../dist/clock.js";

const DATE = {
  year: 3,
  day_of_year: 363,
  month: 12,
  day_of_month: 27,
  week: 51,
  weekday: 6,
  season: 3,
  second_of_day: 86399,
};
const LATER = { ...DATE, year: 4 };

test("a pressed step is asked for once, never two at a time", () => {
  const [busy, asked] = press(standing(10, DATE), "year");
  assert.equal(asked, "year");
  assert.equal(busy.busy, true);
  assert.deepEqual(press(busy, "days28"), [busy, null]);
  const [done, again] = answered(busy, 20, LATER);
  assert.equal(again, null);
  assert.deepEqual(done, { closed: 20, date: LATER, playing: null, busy: false, refused: null });
});

test("playing asks the same step after each answer until a pause", () => {
  let [clock, asked] = play(standing(10, DATE), "decade");
  assert.equal(asked, "decade");
  [clock, asked] = answered(clock, 20, LATER);
  assert.equal(asked, "decade");
  assert.equal(clock.busy, true);
  [clock, asked] = pause(clock);
  assert.equal(asked, null);
  assert.equal(clock.busy, true, "the request on its way still lands");
  [clock, asked] = answered(clock, 30, LATER);
  assert.equal(asked, null);
  assert.equal(clock.closed, 30);
  assert.equal(clock.busy, false);
});

test("playing while a request is on its way waits for its answer", () => {
  let [clock] = press(standing(10, DATE), "year");
  let asked;
  [clock, asked] = play(clock, "days28");
  assert.equal(asked, null);
  [clock, asked] = answered(clock, 20, LATER);
  assert.equal(asked, "days28");
});

test("a refusal ends playing and is shown until the next step", () => {
  let [clock] = play(standing(10, DATE), "year");
  let asked;
  [clock, asked] = refused(clock, "run_locked");
  assert.equal(asked, null);
  assert.deepEqual(clock, {
    closed: 10,
    date: DATE,
    playing: null,
    busy: false,
    refused: "run_locked",
  });
  [clock, asked] = press(clock, "year");
  assert.equal(asked, "year");
  assert.equal(clock.refused, null);
});
