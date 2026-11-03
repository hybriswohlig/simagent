// Where the house page draws rooms, objects and people (ADR-0025 section 5): floors side by
// side, people at their object, in the middle of their room, or part of the way.
import { test } from "node:test";
import assert from "node:assert/strict";

import { FLOOR_GAP, plan, roomMiddle, objectMiddle, SPREAD, spots } from "../dist/render/plan.js";

// A hall of 2 by 4 m and a kitchen of 4 by 4 m beside it on the ground floor; a landing of
// 2 by 3 m upstairs. A table in the kitchen.
const layout = {
  rooms: [
    { id: "hall", floor: 0, x: 0, y: 0, w: 2, h: 4 },
    { id: "kitchen", floor: 0, x: 2, y: 0, w: 4, h: 4 },
    { id: "landing", floor: 1, x: 0, y: 1, w: 2, h: 3 },
  ],
  doors: [],
  objects: [{ kind: "core:dining_table", room: "kitchen", x: 3, y: 1, w: 2, h: 1, slots: 4, offers: ["eat"] }],
};
const second = (t) => ({ t, date: {} });

test("the floors stand side by side", () => {
  const drawn = plan(layout);
  assert.deepEqual(drawn.offsets, [0, 6 + FLOOR_GAP]);
  assert.equal(drawn.width, 6 + FLOOR_GAP + 2);
  assert.equal(drawn.height, 4);
  assert.deepEqual(roomMiddle(layout, drawn, 1), { x: 4, y: 2 });
  assert.deepEqual(roomMiddle(layout, drawn, 2), { x: 6 + FLOOR_GAP + 1, y: 2.5 });
  assert.deepEqual(objectMiddle(layout, drawn, 0), { x: 4, y: 1.5 });
});

test("people at their object, in their room, on the way", () => {
  const at = 100;
  const persons = [
    { sim: 1, place: { room: 1 }, action: { object: 0, offer: 0, slot: 0, until: second(200) }, needs: [] },
    { sim: 2, place: { room: 0 }, action: null, needs: [] },
    // From the hall to the kitchen, from 90 to 110: half the way at 100.
    { sim: 3, place: { walking: { from: 0, to: 1, depart: second(90), arrive: second(110) } }, action: null, needs: [] },
    // A second one at the table stands beside the first.
    { sim: 4, place: { room: 1 }, action: { object: 0, offer: 0, slot: 1, until: second(300) }, needs: [] },
  ];
  assert.deepEqual(spots(layout, persons, at), [
    { x: 4, y: 1.5 },
    { x: 1, y: 2 },
    { x: 2.5, y: 2 },
    { x: 4 + SPREAD, y: 1.5 },
  ]);
  // Before the walk begins and after it ends, at its ends.
  const walking = [persons[2]];
  assert.deepEqual(spots(layout, walking, 80), [{ x: 1, y: 2 }]);
  assert.deepEqual(spots(layout, walking, 120), [{ x: 4, y: 2 }]);
});

test("a second of the day as a clock reads it", async () => {
  const { timeOfDay } = await import("../dist/render/plan.js");
  assert.equal(timeOfDay(0), "0:00");
  assert.equal(timeOfDay(6 * 3600 + 5 * 60 + 59), "6:05");
  assert.equal(timeOfDay(23 * 3600 + 59 * 60 + 59), "23:59");
});
