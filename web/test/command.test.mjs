// The offers the house page can command (ADR-0024 section 5): every offer of every object in
// the core's numbering, by object and then offer, named as a person does it and in which room;
// a name two offers share carries their order among them. The answer gives no ages, so every
// offer is there and the core refuses what an age does not allow. Written by hand from the
// words of `text.ts`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { commandChoices, offerWords } from "../dist/command.js";

/** An object of a drawing; only the kind, the room and the offers matter here. */
function object(kind, room, offers) {
  return { kind, room, x: 0, y: 0, w: 1, h: 1, slots: 1, offers };
}

const LAYOUT = {
  rooms: [],
  doors: [],
  objects: [
    object("core:dining_table", "kitchen", ["eat"]),
    object("core:single_bed", "kids", ["sleep"]),
    object("core:toy_chest", "kids", ["play"]),
    object("core:single_bed", "kids", ["sleep"]),
    object("test:box", "attic", ["poke", "kick"]),
  ],
};

test("every offer of every object, named where it is", () => {
  assert.deepEqual(commandChoices(LAYOUT), [
    { object: 0, offer: 0, label: "isst am Esstisch in der Küche" },
    { object: 1, offer: 0, label: "schläft im Einzelbett im Kinderzimmer (1)" },
    { object: 2, offer: 0, label: "spielt an der Spielzeugkiste im Kinderzimmer" },
    { object: 3, offer: 0, label: "schläft im Einzelbett im Kinderzimmer (2)" },
    { object: 4, offer: 0, label: "poke test:box attic" },
    { object: 4, offer: 1, label: "kick test:box attic" },
  ]);
});

test("what an offer is, as a person does it", () => {
  assert.equal(offerWords(LAYOUT, 0, 0), "isst am Esstisch");
  assert.equal(offerWords(LAYOUT, 4, 1), "kick test:box");
});

test("a house without objects offers nothing", () => {
  assert.deepEqual(commandChoices({ rooms: [], doors: [], objects: [] }), []);
});
