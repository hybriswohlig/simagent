// The chronicle of a run page keeps the entries a player follows, newest first, each once,
// at most a fixed number (ADR-0021 section 2: the log in the order of the file).
import { test } from "node:test";
import assert from "node:assert/strict";

import { KEPT, added, payloadCode, segments } from "../dist/chronicle.js";

const DATE = {
  year: 1,
  day_of_year: 2,
  month: 0,
  day_of_month: 2,
  week: 0,
  weekday: 2,
  season: 0,
  second_of_day: 3,
};

/** An item of a page; the values are arbitrary, only the shape follows the format. */
function item(id, code) {
  return {
    entry: { id, t: id, subjects: [{ sim: 1 }], causes: [{ event: 1 }], payload: { [code]: {} } },
    date: DATE,
  };
}

test("an entry's code is the one key of its payload", () => {
  assert.equal(payloadCode(item(1, "core:birth")), "core:birth");
  const two = { ...item(2, "core:birth"), entry: { ...item(2, "x").entry, payload: { a: {}, b: {} } } };
  assert.equal(payloadCode(two), null);
});

test("only followed payloads are kept, newest first", () => {
  const page = [
    item(1, "core:birth"),
    item(2, "core:birthday"),
    item(3, "core:meiosis"),
    item(4, "core:death"),
    item(5, "core:conception"),
    item(6, "core:partnered"),
  ];
  const kept = added([], page);
  assert.deepEqual(
    kept.map((i) => i.entry.id),
    [6, 4, 1],
  );
});

test("a page that overlaps the kept adds each entry once", () => {
  const kept = added([], [item(1, "core:birth"), item(2, "core:death")]);
  const next = added(kept, [item(2, "core:death"), item(3, "core:moved")]);
  assert.deepEqual(
    next.map((i) => i.entry.id),
    [3, 2, 1],
  );
});

test("the chronicle keeps at most its limit, the newest", () => {
  const page = Array.from({ length: KEPT + 5 }, (_, i) => item(i + 1, "core:birth"));
  const kept = added([], page);
  assert.equal(kept.length, KEPT);
  assert.equal(kept[0]?.entry.id, KEPT + 5);
  assert.equal(kept.at(-1)?.entry.id, 6);
});

test("a template puts the persons in their roles, all members as a list", () => {
  assert.deepEqual(segments("{0} wird geboren, Mutter {1}, Vater {2}", [9, 4, 5]), [
    9,
    " wird geboren, Mutter ",
    4,
    ", Vater ",
    5,
  ]);
  assert.deepEqual(segments("Neuer Haushalt: {all}", [1]), ["Neuer Haushalt: ", 1]);
  assert.deepEqual(segments("Neuer Haushalt: {all}", [1, 2]), ["Neuer Haushalt: ", 1, " und ", 2]);
  assert.deepEqual(segments("Neuer Haushalt: {all}", [1, 2, 3]), [
    "Neuer Haushalt: ",
    1,
    ", ",
    2,
    " und ",
    3,
  ]);
  assert.deepEqual(segments("Die Welt beginnt.", []), ["Die Welt beginnt."]);
  assert.deepEqual(segments("{0} und {1}", [7]), [7, " und ", null], "a role without a person");
});
