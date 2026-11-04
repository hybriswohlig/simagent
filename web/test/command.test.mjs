// The offers the house page can command (ADR-0024 section 5): every offer of every object in
// the core's numbering, by object and then offer, named as a person does it and in which room;
// a name two offers share carries their order among them. Only those the house answer allows
// the person (version 5), with the same numbers. Objects told apart from others of their kind
// by their room, and within a room by their order (the playtest of 2026-11-04). Written by hand
// from the words of `text.ts`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { commandChoices, objectName, offerWords, placedOfferWords, whereObject } from "../dist/command.js";

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

test("only the offers the server allows, numbered as among all", () => {
  const allowed = [
    { object: 3, offer: 0 },
    { object: 0, offer: 0 },
    { object: 4, offer: 1 },
  ];
  assert.deepEqual(commandChoices(LAYOUT, allowed), [
    { object: 0, offer: 0, label: "isst am Esstisch in der Küche" },
    { object: 3, offer: 0, label: "schläft im Einzelbett im Kinderzimmer (2)" },
    { object: 4, offer: 1, label: "kick test:box attic" },
  ]);
  assert.deepEqual(commandChoices(LAYOUT, []), []);
});

test("what an offer is, as a person does it", () => {
  assert.equal(offerWords(LAYOUT, 0, 0), "isst am Esstisch");
  assert.equal(offerWords(LAYOUT, 4, 1), "kick test:box");
});

test("a house without objects offers nothing", () => {
  assert.deepEqual(commandChoices({ rooms: [], doors: [], objects: [] }), []);
});

// The terraced house: a toilet in the WC and one in the bath, two single beds in the children's
// room and one in the small room, one shower.
const TERRACED = {
  rooms: [],
  doors: [],
  objects: [
    object("core:toilet", "wc", ["use"]),
    object("core:toilet", "bath", ["use"]),
    object("core:shower", "bath", ["shower"]),
    object("core:single_bed", "kids", ["sleep"]),
    object("core:single_bed", "kids", ["sleep"]),
    object("core:single_bed", "small_room", ["sleep"]),
  ],
};

test("an object told apart from others of its kind", () => {
  assert.deepEqual(
    TERRACED.objects.map((_, index) => whereObject(TERRACED, index)),
    [" auf dem WC", " im Bad", "", " im Kinderzimmer (1)", " im Kinderzimmer (2)", " im kleinen Zimmer"],
  );
  assert.deepEqual(
    TERRACED.objects.map((_, index) => objectName(TERRACED, index)),
    ["Toilette auf dem WC", "Toilette im Bad", "Dusche", "Einzelbett im Kinderzimmer (1)", "Einzelbett im Kinderzimmer (2)", "Einzelbett im kleinen Zimmer"],
  );
  assert.equal(placedOfferWords(TERRACED, 1, 0), "ist auf der Toilette im Bad");
  assert.equal(placedOfferWords(TERRACED, 2, 0), "duscht unter der Dusche");
  assert.equal(placedOfferWords(TERRACED, 4, 0), "schläft im Einzelbett im Kinderzimmer (2)");
  assert.equal(whereObject(TERRACED, 9), "");
});
