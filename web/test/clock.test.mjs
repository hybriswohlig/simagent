// The clock of a run page asks for one step at a time; playing asks for the same step after
// each answer, at the pace of its ticks, until a pause or a refusal (ADR-0021 section 2).
// Pausing is no input.
import { test } from "node:test";
import assert from "node:assert/strict";

import { answered, heard, pause, play, playText, press, refused, standing, tick } from "../dist/clock.js";

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

test("playing asks the same step at each tick after an answer, until a pause", () => {
  let [clock, asked] = play(standing(10, DATE), "decade");
  assert.equal(asked, "decade", "at once");
  assert.deepEqual(tick(clock), [clock, null], "not while a request is on its way");
  [clock, asked] = answered(clock, 20, LATER);
  assert.equal(asked, null, "the player sees the step first");
  assert.equal(clock.busy, false);
  [clock, asked] = tick(clock);
  assert.equal(asked, "decade");
  [clock, asked] = pause(clock);
  assert.equal(asked, null);
  assert.equal(clock.busy, true, "the request on its way still lands");
  [clock, asked] = answered(clock, 30, LATER);
  assert.equal(clock.closed, 30);
  assert.deepEqual(tick(clock), [clock, null], "paused");
});

test("playing while a request is on its way waits for its answer and the tick", () => {
  let [clock] = press(standing(10, DATE), "year");
  let asked;
  [clock, asked] = play(clock, "days28");
  assert.equal(asked, null);
  [clock, asked] = answered(clock, 20, LATER);
  assert.equal(asked, null);
  [clock, asked] = tick(clock);
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
  assert.deepEqual(tick(clock), [clock, null], "playing ended");
  [clock, asked] = press(clock, "year");
  assert.equal(asked, "year");
  assert.equal(clock.refused, null);
});

// The third playtest (K6): the server came back and the chronicle was read again, yet "Der
// Server antwortet nicht." stayed. An answer of the server ends a refusal for no answer; a
// refusal the server gave stays until the next step.
test("an answer of the server ends a refusal for no answer, and no other", () => {
  const [down] = refused(standing(10, DATE), "network");
  assert.deepEqual(heard(down), { ...down, refused: null });
  const [locked] = refused(standing(10, DATE), "run_locked");
  assert.deepEqual(heard(locked), locked);
  const [busy] = press(standing(10, DATE), "year");
  assert.deepEqual(heard(busy), busy);
});

test("a step pressed while playing becomes the playing step", () => {
  let [clock] = play(standing(10, DATE), "year");
  let asked;
  [clock] = answered(clock, 20, LATER);
  [clock, asked] = press(clock, "decade");
  assert.equal(asked, "decade");
  assert.equal(clock.playing, "decade");
  [clock] = answered(clock, 30, LATER);
  [clock, asked] = tick(clock);
  assert.equal(asked, "decade");
  const [pressed] = press(standing(10, DATE), "year");
  assert.equal(pressed.playing, null, "pressing alone does not play");
});

// The second playtest: nothing said which step "Abspielen" plays. Its button names the step.
test("the play button names the step it plays", () => {
  assert.equal(playText("days28"), "Abspielen, je 28 Tage");
  assert.equal(playText("year"), "Abspielen, je ein Jahr");
  assert.equal(playText("decade"), "Abspielen, je zehn Jahre");
});
