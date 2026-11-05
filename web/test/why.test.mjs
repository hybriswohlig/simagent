// "Warum?" as the house page tells it (ADR-0026 section 6 and its addenda of 2026-11-04; the
// playtest of 2026-11-04): the mood beside the points with its sign, the typographic minus in
// negative points, an urgent offer by its critical needs instead of its points, a command as
// told, a taken offer waited for once, the objects told apart, and the comparison at the clock
// of an idle choice that is still the person's state; a time-filler, an offer the departure
// closes and a need at its comfort (ADR-0026, addendum of 2026-11-04 on comfort and the
// time-filler); a time-filler's mood and why it ends, one not taken as too short, and a command
// the departure closes (its addendum of 2026-11-05 on the time-filler's length). Written by hand
// from the words of `text.ts`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { beforeBed, fillerWords, moodWords, pointsText, satedWords, shortWords, urgentWords, weightWords, whyText } from "../dist/why.js";

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
    object("core:single_bed", "kids", ["sleep"]),
  ],
};

const date = (second_of_day) => ({ year: 1, day_of_year: 0, month: 0, day_of_month: 0, week: 0, weekday: 0, season: 0, second_of_day });
const dated = (second) => ({ t: second, date: date(second) });
const offer = (object, points, mood, extra = {}) => ({
  object, offer: 0, points, mood, busy: false, closed: false, departs: null, urgent: [], sated: [], held: false, outside: false, bed_for: null, ...extra,
});
const night = { from: dated(79_200), until: dated(21_600), now: false };

function why(fields) {
  return {
    format: "simagent.view.why", version: 7, entry: 1, sim: 1, at: dated(61_200), enough: 40_000, commanded: false,
    needs: [600, 900, 500, 50, 800], comfort: [700, 1000, 700, 900, 1000], night, chosen: null, waits: null, goes_to: null,
    filler: null, too_short: null, offers: [], now: null, ...fields,
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
        { object: 4, offer: 0, points: null, mood: null, busy: false, closed: true, departs: null, urgent: [], sated: [], held: false, outside: false, bed_for: "couples" },
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
  assert.equal(told.told, "Nachtruhe bis 6:00: erst was dringend ist, dann Hunger oder Blase unter 700, dann ein Bett. Gewählt: schläft im Bett, 350.464 Punkte (Laune 0 %).");
});

// The rules after the playtest (ADR-0026, addendum of 2026-11-04 after the playtest).
const NIGHT = { from: dated(79_200), until: dated(21_600), now: true };

test("a bed a small child holds, and a bed of another tier", () => {
  const held = whyText(
    why({
      night: NIGHT,
      chosen: { object: 4, offer: 0, points: 350_464, mood: 0, urgent: [] },
      offers: [offer(4, 350_464, 0, { bed_for: "couples" }), offer(5, 9_000, 1, { busy: true, held: true, bed_for: "children" })],
    }),
    LAYOUT,
  );
  assert.equal(
    held.told,
    "Nachtruhe bis 6:00: erst was dringend ist, dann Hunger oder Blase unter 700, dann ein Bett. Gewählt: schläft im Bett, 350.464 Punkte (Laune 0 %). " +
      "Dahinter: schläft im Einzelbett 9.000 (Laune +1 %) (besetzt) (hier schläft ein Kind).",
  );
  const child = whyText(
    why({
      night: NIGHT,
      chosen: { object: 5, offer: 0, points: 300_000, mood: 0, urgent: [] },
      offers: [offer(5, 300_000, 0, { bed_for: "children" }), offer(4, 299_000, -1, { outside: true, bed_for: "couples" })],
    }),
    LAYOUT,
  );
  assert.equal(
    child.told,
    "Nachtruhe bis 6:00: erst was dringend ist, dann Hunger oder Blase unter 700, dann ein Bett. Gewählt: schläft im Einzelbett, 300.000 Punkte (Laune 0 %). " +
      "Dahinter: schläft im Bett 299.000 (Laune −1 %) (das Bett eines Paares).",
  );
});

test("idle in a room one does not stay in, the room it went to", () => {
  const rooms = [{ id: "living", floor: 0, x: 0, y: 0, w: 4, h: 4 }];
  const told = whyText(why({ goes_to: 0, offers: [offer(3, 38_710, 2)] }), { ...LAYOUT, rooms });
  assert.equal(
    told.told,
    "Nichts brachte genug (40.000 Punkte nötig). Hier bleibt niemand stehen: geht ins Wohnzimmer. Dahinter: sieht fern vor dem Fernseher 38.710 (Laune +2 %).",
  );
});

// Comfort and the time-filler (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler).
const LIVING = { ...LAYOUT, rooms: [{ id: "living", floor: 0, x: 0, y: 0, w: 4, h: 4 }] };

test("a time-filler with its end, what then brings enough, and its walk", () => {
  const filler = { walk_secs: 3, until: dated(37_440), ends: "choice", then: ["bladder"], mood: null, others: [] };
  const told = whyText(
    why({
      chosen: { object: 3, offer: 0, points: 39_000, mood: -2, urgent: [] },
      filler,
      offers: [offer(3, 39_000, -2), offer(0, 38_000, 1)],
    }),
    LIVING,
  );
  assert.equal(
    told.told,
    "Nichts brachte genug (40.000 Punkte nötig). Zeitvertreib: sieht fern vor dem Fernseher, bis 10:24, dann reicht die Blase. " +
      "3 s Weg ins Wohnzimmer. Dahinter: ist auf der Toilette auf dem WC 38.000 (Laune +1 %).",
  );
  const chosen = { object: 3, offer: 0, points: null, mood: null, urgent: [] };
  const running = fillerWords(why({ chosen, filler: { walk_secs: 5, until: null, ends: null, then: [], mood: null, others: [] } }), LIVING);
  assert.equal(running, "Zeitvertreib: sieht fern vor dem Fernseher. 5 s Weg ins Wohnzimmer.");
  const both = { walk_secs: 5, until: dated(36_000), ends: "choice", then: ["hunger", "bladder"], mood: null, others: [] };
  const two = fillerWords(why({ chosen, filler: both }), LIVING);
  assert.equal(two, "Zeitvertreib: sieht fern vor dem Fernseher, bis 10:00, dann reichen der Hunger und die Blase. 5 s Weg ins Wohnzimmer.");
  assert.equal(fillerWords(why({}), LIVING), "");
});

test("an offer the departure closes, with the departure", () => {
  const closes = { points: null, mood: null, closed: true, departs: dated(31_200) };
  const told = whyText(why({ offers: [offer(0, 38_000, 1), offer(2, null, null, closes)] }), LAYOUT);
  assert.equal(
    told.told,
    "Nichts brachte genug (40.000 Punkte nötig). Dahinter: ist auf der Toilette auf dem WC 38.000 (Laune +1 %), " +
      "duscht unter der Dusche endet nach dem Aufbruch um 8:40.",
  );
});

test("a need at or above its comfort brings nothing", () => {
  const needs = [750, 900, 500, 950, 800];
  const comfort = [700, 1000, 700, 900, 1000];
  assert.equal(satedWords(offer(0, -250, 0, { sated: ["hunger"] }), needs, comfort), " (Hunger zählt ab 700 und steht bei 750, das bringt nichts)");
  assert.equal(satedWords(offer(0, -250, 0), needs, comfort), "");
  const told = whyText(why({ needs, offers: [offer(2, -150, 0, { sated: ["hygiene"] })] }), LAYOUT);
  assert.equal(
    told.told,
    "Nichts brachte genug (40.000 Punkte nötig). Dahinter: duscht unter der Dusche −150 (Laune 0 %) (Hygiene zählt ab 900 und steht bei 950, das bringt nichts).",
  );
});

test("a need that weighs nothing now says from where it counts", () => {
  assert.equal(weightWords(0, 700), " · zählt ab 700");
  assert.equal(weightWords(50, 700), "");
  assert.equal(weightWords(0, 1000), "");
  assert.equal(weightWords(undefined, 700), "");
});

// The time-filler's length (ADR-0026, addendum of 2026-11-05 on the time-filler's length).
const READING = {
  rooms: [{ id: "living", floor: 0, x: 0, y: 0, w: 4, h: 4 }],
  doors: [],
  objects: [
    object("core:bookshelf", "living", ["read"]),
    object("core:tv", "living", ["watch"]),
    object("core:sofa", "living", ["sleep", "sit"]),
    object("core:toilet", "wc", ["use"]),
  ],
};

test("a time-filler with its mood, the others' and why it ends", () => {
  const chosen = { object: 0, offer: 0, points: 20_000, mood: 8, departs: null, urgent: [] };
  const others = [{ object: 1, offer: 0, mood: 2 }, { object: 2, offer: 1, mood: -4 }];
  const filler = (ends, until) => ({ walk_secs: 3, until: dated(until), ends, then: [], mood: 8, others });
  assert.equal(
    fillerWords(why({ chosen, filler: filler("time", 69_120) }), READING),
    "Zeitvertreib: liest am Bücherregal, Laune +8 % (Fernseher +2 %, Sofa −4 %), bis 19:12, dann ist die Zeit um. 3 s Weg ins Wohnzimmer.",
  );
  assert.equal(
    fillerWords(why({ chosen, filler: filler("bedtime", 79_200) }), READING),
    "Zeitvertreib: liest am Bücherregal, Laune +8 % (Fernseher +2 %, Sofa −4 %), bis 22:00, dann ist Schlafenszeit. 3 s Weg ins Wohnzimmer.",
  );
  assert.equal(
    fillerWords(why({ chosen, filler: filler("departure", 31_200) }), READING),
    "Zeitvertreib: liest am Bücherregal, Laune +8 % (Fernseher +2 %, Sofa −4 %), bis 8:40, dann ist Aufbruch. 3 s Weg ins Wohnzimmer.",
  );
  const alone = { walk_secs: 3, until: dated(37_440), ends: "choice", then: ["bladder"], mood: 0, others: [] };
  assert.equal(
    fillerWords(why({ chosen, filler: alone }), READING),
    "Zeitvertreib: liest am Bücherregal, Laune 0 %, bis 10:24, dann reicht die Blase. 3 s Weg ins Wohnzimmer.",
  );
  const told = whyText(why({ chosen, filler: filler("time", 69_120), offers: [offer(3, 30_000, 1)] }), READING);
  assert.equal(
    told.told,
    "Nichts brachte genug (40.000 Punkte nötig). Zeitvertreib: liest am Bücherregal, Laune +8 % (Fernseher +2 %, Sofa −4 %), " +
      "bis 19:12, dann ist die Zeit um. 3 s Weg ins Wohnzimmer. Dahinter: ist auf der Toilette 30.000 (Laune +1 %).",
  );
});

test("no time-filler when it would end too soon", () => {
  const short = (ends, until) => why({ too_short: { until: dated(until), ends }, offers: [offer(3, 30_000, 1)] });
  assert.equal(shortWords(short("departure", 27_600)), " Zu kurz vor dem Aufbruch um 7:40.");
  assert.equal(shortWords(short("bedtime", 72_000)), " Zu kurz vor der Schlafenszeit um 20:00.");
  assert.equal(shortWords(short("choice", 37_440)), " Zu kurz vor der nächsten Wahl um 10:24.");
  assert.equal(shortWords(short("time", 37_440)), " Zu kurz: ein Zeitvertreib wäre um 10:24 vorbei.");
  assert.equal(shortWords(why({})), "");
  assert.equal(
    whyText(short("departure", 27_600), READING).told,
    "Nichts brachte genug (40.000 Punkte nötig). Zu kurz vor dem Aufbruch um 7:40. Dahinter: ist auf der Toilette 30.000 (Laune +1 %).",
  );
});

test("a command the departure closes, without points", () => {
  const chosen = { object: 3, offer: 0, points: null, mood: null, departs: dated(31_200), urgent: [] };
  const told = whyText(why({ commanded: true, chosen, offers: [offer(0, 39_850, null)] }), LAYOUT);
  assert.equal(
    told.told,
    "Befohlen: sieht fern vor dem Fernseher. Endet nach dem Aufbruch um 8:40, darum ohne Punkte. Dahinter: ist auf der Toilette auf dem WC 39.850.",
  );
});

test("the night looks after hunger and the bladder under their comfort before a bed", () => {
  assert.equal(beforeBed([700, 1000, 700, 900, 1000]), "Hunger oder Blase unter 700");
  assert.equal(beforeBed([650, 1000, 700, 900, 1000]), "Hunger unter 650 oder Blase unter 700");
  const last = whyText(
    why({ night: { from: dated(79_200), until: null, now: true }, chosen: { object: 4, offer: 0, points: 350_464, mood: 0, urgent: [] }, offers: [offer(4, 350_464, 0)] }),
    LAYOUT,
  );
  assert.equal(
    last.told,
    "Nachtruhe bis zum Ende der Zeit: erst was dringend ist, dann Hunger oder Blase unter 700, dann ein Bett. Gewählt: schläft im Bett, 350.464 Punkte (Laune 0 %).",
  );
});
