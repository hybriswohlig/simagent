// "Warum?" as the house page tells it (ADR-0026 section 6 and its addenda of 2026-11-04; the
// playtest of 2026-11-04): the mood beside the points with its sign, the typographic minus in
// negative points, an urgent offer by its critical needs instead of its points, a command as
// told, a taken offer waited for once, the objects told apart, and the comparison at the clock
// of an idle choice that is still the person's state. Written by hand from the words of
// `text.ts`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { moodWords, pointsText, urgentWords, whyText } from "../dist/why.js";

test("the mood with its sign, nothing without one", () => {
  assert.equal(moodWords(6), " (Laune +6 %)");
  assert.equal(moodWords(10), " (Laune +10 %)");
  assert.equal(moodWords(-3), " (Laune −3 %)");
  assert.equal(moodWords(-10), " (Laune −10 %)");
  assert.equal(moodWords(0), " (Laune 0 %)");
  assert.equal(moodWords(null), "");
});

test("points as German numbers, the minus typographic", () => {
  assert.equal(pointsText(269_900), "269.900");
  assert.equal(pointsText(-250), "−250");
  assert.equal(pointsText(0), "0");
  assert.equal(pointsText(-1_234_567), "−1.234.567");
});

test("what is urgent, with the values of the choice", () => {
  const needs = [100, 900, 120, 50, 800];
  assert.equal(urgentWords(["hygiene"], needs), "dringend (Hygiene 50)");
  assert.equal(urgentWords(["hunger", "bladder"], needs), "dringend (Hunger 100, Blase 120)");
});

function object(kind, room, offers) {
  return { kind, room, x: 0, y: 0, w: 1, h: 1, slots: 1, offers };
}

// A WC and a bath with a toilet each, a shower in the bath, a television.
const LAYOUT = {
  rooms: [],
  doors: [],
  objects: [
    object("core:toilet", "wc", ["use"]),
    object("core:toilet", "bath", ["use"]),
    object("core:shower", "bath", ["shower"]),
    object("core:tv", "living", ["watch"]),
    object("core:bed", "bedroom", ["sleep"]),
  ],
};

const date = (second_of_day) => ({ year: 1, day_of_year: 0, month: 0, day_of_month: 0, week: 0, weekday: 0, season: 0, second_of_day });
const dated = (second) => ({ t: second, date: date(second) });
const offer = (object, points, mood, extra = {}) => ({ object, offer: 0, points, mood, busy: false, closed: false, urgent: [], ...extra });
const night = { from: dated(79_200), until: dated(21_600), now: false };

function why(fields) {
  return {
    format: "simagent.view.why", version: 4, entry: 1, sim: 1, at: dated(61_200), enough: 40_000, commanded: false,
    needs: [600, 900, 500, 50, 800], night, chosen: null, waits: null, offers: [], now: null, ...fields,
  };
}

test("an urgent choice by its need, the others by their points", () => {
  const told = whyText(
    why({
      chosen: { object: 2, offer: 0, points: 101_000_631_000, mood: -5, urgent: ["hygiene"] },
      offers: [
        offer(2, 101_000_631_000, -5, { urgent: ["hygiene"] }),
        offer(0, 269_900, 8),
        offer(1, -250, 3, { busy: true }),
        { object: 4, offer: 0, points: null, mood: null, busy: false, closed: true, urgent: [] },
      ],
    }),
    LAYOUT,
  );
  assert.deepEqual(told, {
    told:
      "Gewählt: duscht unter der Dusche, dringend (Hygiene 50). Dahinter: ist auf der Toilette auf dem WC 269.900 (Laune +8 %), " +
      "ist auf der Toilette im Bad −250 (Laune +3 %) (besetzt), schläft im Bett zu, ab 22:00.",
    now: null,
  });
});

test("a command is told so, its points without a mood", () => {
  const told = whyText(
    why({ commanded: true, chosen: { object: 3, offer: 0, points: -250, mood: null, urgent: [] }, offers: [offer(3, -250, null), offer(0, 39_850, null)] }),
    LAYOUT,
  );
  assert.equal(told.told, "Befohlen: sieht fern vor dem Fernseher, −250 Punkte. Dahinter: ist auf der Toilette auf dem WC 39.850.");
});

test("a taken offer waited for stands once", () => {
  const told = whyText(
    why({ waits: 2, offers: [offer(2, 107_309, -3, { busy: true }), offer(3, 39_000, 1)] }),
    LAYOUT,
  );
  assert.equal(
    told.told,
    "Das Beste war besetzt: duscht unter der Dusche, 107.309 Punkte (Laune −3 %); darauf wird gewartet. Dahinter: sieht fern vor dem Fernseher 39.000 (Laune +1 %).",
  );
});

test("an idle choice, and the same comparison at the clock", () => {
  const told = whyText(
    why({
      offers: [offer(3, 38_710, 2)],
      now: { at: dated(59_400), needs: [500, 800, 400, 600, 300], offers: [offer(3, 55_000, -1), offer(1, 41_000, 4, { busy: true })] },
    }),
    LAYOUT,
  );
  assert.deepEqual(told, {
    told: "Nichts brachte genug (40.000 Punkte nötig). Dahinter: sieht fern vor dem Fernseher 38.710 (Laune +2 %).",
    now: "Jetzt, um 16:30: sieht fern vor dem Fernseher 55.000 (Laune −1 %), ist auf der Toilette im Bad 41.000 (Laune +4 %) (besetzt).",
  });
});

test("in the night the bed first", () => {
  const told = whyText(
    why({ night: { from: dated(79_200), until: dated(21_600), now: true }, chosen: { object: 4, offer: 0, points: 350_464, mood: 0, urgent: [] }, offers: [offer(4, 350_464, 0)] }),
    LAYOUT,
  );
  assert.equal(told.told, "Nachtruhe bis 6:00: erst was dringend ist, dann ein Bett. Gewählt: schläft im Bett, 350.464 Punkte (Laune 0 %).");
});
