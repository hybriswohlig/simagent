// The lines of a person's day on the house page (the playtest of 2026-11-04): a choice and a
// command with the object told apart from others of its kind, what ended and how ("isst fertig
// am Esstisch", "wacht auf", cut off with its reason), where an idle person stands, what it
// waits for; and the day list by calendar day across midnight. A person's line now: what it
// waits for and where, and an action the departure ends at its time (ADR-0021, addendum of
// 2026-11-05 on the house page). Written by hand from the words of `text.ts`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { dayGroups, dayLine, doing } from "../dist/day.js";

function object(kind, room, offers) {
  return { kind, room, x: 0, y: 0, w: 1, h: 1, slots: 1, offers };
}

const LAYOUT = {
  rooms: [
    { id: "hall", floor: 0, x: 0, y: 0, w: 2, h: 6 },
    { id: "kitchen", floor: 0, x: 2, y: 0, w: 4, h: 4 },
    { id: "wc", floor: 0, x: 0, y: 6, w: 2, h: 1.5 },
    { id: "bath", floor: 1, x: 0, y: 0, w: 2, h: 3 },
  ],
  doors: [],
  objects: [
    object("core:dining_table", "kitchen", ["eat"]),
    object("core:toilet", "wc", ["use"]),
    object("core:toilet", "bath", ["use"]),
    object("core:bed", "bath", ["sleep"]),
    object("test:box", "hall", ["poke"]),
  ],
};

const date = (weekday, day_of_year, second_of_day) => ({ year: 20, day_of_year, month: 2, day_of_month: day_of_year, week: 0, weekday, season: 0, second_of_day });
const item = (id, code, fields, when = date(0, 5, 0)) => ({
  entry: { id, t: 0, subjects: [{ sim: 1 }], causes: [], payload: { [code]: fields } },
  date: when,
});
const line = (code, fields) => dayLine(LAYOUT, item(1, code, fields));

test("a choice and a command, the object told apart", () => {
  assert.deepEqual(line("core:action_started", { object: 2, offer: 0, commanded: false }), { text: "Wahl: ist auf der Toilette im Bad", choice: true });
  assert.deepEqual(line("core:action_started", { object: 0, offer: 0, commanded: true }), { text: "Befehl: isst am Esstisch", choice: true });
  // A time-filler says so (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler).
  assert.deepEqual(
    line("core:action_started", { object: 2, offer: 0, commanded: false, filler: true }),
    { text: "Zeitvertreib: ist auf der Toilette im Bad", choice: true },
  );
});

test("what ended and how", () => {
  const ended = (object, reason) => line("core:action_ended", { object, offer: 0, reason, needs: [] }).text;
  assert.equal(ended(0, "done"), "isst fertig am Esstisch");
  assert.equal(ended(1, "full"), "ist fertig auf der Toilette auf dem WC");
  assert.equal(ended(3, "done"), "wacht auf");
  assert.equal(ended(3, "critical"), "wacht auf (Dringenderes)");
  assert.equal(ended(0, "command"), "hört auf: isst am Esstisch (Befehl)");
  assert.equal(ended(2, "obligation"), "hört auf: ist auf der Toilette im Bad (Pflicht)");
  // An offer without its finished words is told as cut off, with the reason.
  assert.equal(ended(4, "done"), "hört auf: poke test:box (fertig)");
  assert.equal(line("core:action_ended", { object: 0, offer: 0, reason: "done" }).choice, false);
});

test("idle where, waiting for what", () => {
  assert.deepEqual(line("core:idle", { room: 3, needs: [], busy: [], waits: null }), { text: "tut nichts im Bad", choice: true });
  assert.deepEqual(line("core:idle", { room: 0, needs: [], busy: [1], waits: 1 }), { text: "wartet auf: Toilette auf dem WC", choice: true });
  assert.equal(line("core:reached", { object: 0 }), null);
});

test("the day list by calendar day", () => {
  const items = [
    item(1, "core:idle", {}, date(0, 5, 79_000)),
    item(2, "core:idle", {}, date(0, 5, 82_000)),
    item(3, "core:idle", {}, date(1, 6, 21_600)),
  ];
  const groups = dayGroups(items);
  assert.deepEqual(
    groups.map((group) => [group.title, group.items.map((each) => each.entry.id)]),
    [
      ["Montag, Jahr 20, 3. Monat, 6. Tag", [1, 2]],
      ["Dienstag, Jahr 20, 3. Monat, 7. Tag", [3]],
    ],
  );
  assert.deepEqual(dayGroups([]), []);
});

// A hall, a bath with a toilet and a shower, a living room with a television, a WC with a toilet.
const HOME = {
  rooms: [
    { id: "hall", floor: 0, x: 0, y: 0, w: 2, h: 6 },
    { id: "bath", floor: 1, x: 0, y: 0, w: 2, h: 3 },
    { id: "living", floor: 0, x: 2, y: 0, w: 4, h: 4 },
    { id: "wc", floor: 0, x: 0, y: 6, w: 2, h: 1.5 },
  ],
  doors: [],
  objects: [
    object("core:toilet", "wc", ["use"]),
    object("core:toilet", "bath", ["use"]),
    object("core:shower", "bath", ["shower"]),
    object("core:tv", "living", ["watch"]),
  ],
};

const person = (fields) => ({
  sim: 1, place: { room: 0 }, action: null, waits: null, needs: [600, 900, 500, 700, 800], weights: [100, 100, 200, 200, 200],
  collapsed: false, smells: false, commands: [], ...fields,
});
const dated = (second_of_day) => ({ t: second_of_day, date: date(1, 6, second_of_day) });
const watching = (fields) => ({ object: 3, offer: 0, slot: 0, until: dated(32_700), filler: false, departs: false, ...fields });

test("a person's line: waiting for what and where, idle where", () => {
  assert.equal(doing(HOME, person({ place: { room: 1 }, waits: 2 })), "wartet im Bad auf die Dusche");
  assert.equal(doing(HOME, person({ waits: 0 })), "wartet im Flur auf die Toilette auf dem WC");
  assert.equal(doing(HOME, person({ place: { room: 1 }, waits: 1 })), "wartet im Bad auf die Toilette im Bad");
  assert.equal(doing(HOME, person({})), "tut gerade nichts im Flur");
});

test("a person's line: until when, the departure that ends it named", () => {
  const at = { place: { room: 2 } };
  assert.equal(doing(HOME, person({ ...at, action: watching({}) })), "sieht fern vor dem Fernseher bis 9:05");
  assert.equal(
    doing(HOME, person({ ...at, action: watching({ until: dated(31_200), departs: true }) })),
    "sieht fern vor dem Fernseher bis 8:40 (Aufbruch)",
  );
  assert.equal(
    doing(HOME, person({ ...at, action: watching({ until: dated(31_200), filler: true, departs: true }) })),
    "Zeitvertreib: sieht fern vor dem Fernseher bis 8:40 (Aufbruch)",
  );
  const walking = { walking: { from: 0, to: 2, depart: dated(30_000), arrive: dated(30_003), out: false } };
  assert.equal(doing(HOME, person({ place: walking, action: watching({ until: null }) })), "geht ins Wohnzimmer");
  assert.equal(doing(HOME, person({ place: { ...walking, walking: { ...walking.walking, out: true } } })), "geht aus dem Haus");
  assert.equal(doing(HOME, person({ place: { away: { place: "work", until: dated(62_400) } } })), "bei der Arbeit bis 17:20");
  assert.equal(doing(HOME, person({ collapsed: true })), "liegt erschöpft am Boden im Flur");
});
