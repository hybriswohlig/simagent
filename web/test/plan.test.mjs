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

test("children under 6 in the middle of their room, off the furniture", () => {
  // A child has no action (ADR-0024 section 4): in the middle of its room, beside a person
  // there who does nothing, asleep or awake alike. Not on a piece of furniture (the fourth
  // playtest): the kitchen's middle (4, 2) touches the table (3 to 5 by 1 to 2), and so do the
  // places beside it, (4.65, 2) and (3.35, 2); below it, (4, 2.65), the dot of 0.3 m keeps
  // 0.65 m from the table.
  const drawn = [
    { sim: 2, place: { room: 0 }, action: null, needs: [] },
    { sim: 5, place: { room: 0, asleep: false } },
    { sim: 6, place: { room: 1, asleep: true } },
  ];
  assert.deepEqual(spots(layout, drawn, 0), [
    { x: 1, y: 2 },
    { x: 1 + SPREAD, y: 2 },
    { x: 4, y: 2 + SPREAD },
  ]);
});

test("a second of the day as a clock reads it", async () => {
  const { timeOfDay } = await import("../dist/render/plan.js");
  assert.equal(timeOfDay(0), "0:00");
  assert.equal(timeOfDay(6 * 3600 + 5 * 60 + 59), "6:05");
  assert.equal(timeOfDay(23 * 3600 + 59 * 60 + 59), "23:59");
  // A late departure in whole minutes (ADR-0024 section 4): 1 295 s are 21 minutes.
  const { wholeMinutes } = await import("../dist/render/plan.js");
  assert.equal(wholeMinutes(1_295), 21);
  assert.equal(wholeMinutes(59), 0);
  assert.equal(wholeMinutes(60), 1);
});

test("nobody covers another, also near one another", () => {
  // The kids' room of the terraced house upstairs: a single bed at 1.6 to 2.5 by 4.3 to 6.3,
  // its middle 0.21 m from the room's (4, 5.5) in the plan. One at the bed stands on it. A child
  // in the room stands off the bed (the fourth playtest): its middle and the eight places one
  // step around it lie on the bed or nearer to it than the dot's 0.3 m (to the right 0.15, to
  // the left 0.25), so two steps to the right, (5.3, 5.5), 0.8 m from the bed. Five children:
  // then two steps to the left (0.9 m from the bed); two steps down and up leave the room (its
  // wall 0.2 m away); so two to the right and one down, two to the left and one down, two to the
  // right and one up, each a step from the one before.
  const kids = {
    rooms: [{ id: "kids", floor: 0, x: 2, y: 4, w: 4, h: 3 }],
    doors: [],
    objects: [{ kind: "core:single_bed", room: "kids", x: 3.6, y: 4.3, w: 0.9, h: 2, slots: 1, offers: ["sleep"] }],
  };
  const sleeper = { place: { room: 0 }, action: { object: 0 } };
  const child = { place: { room: 0 } };
  assert.deepEqual(spots(kids, [sleeper, child], 0), [
    { x: 3.6 + 0.9 / 2, y: 4.3 + 2 / 2 },
    { x: 4 + 2 * SPREAD, y: 5.5 },
  ]);
  assert.deepEqual(spots(kids, [child, child, child, child, child], 0), [
    { x: 4 + 2 * SPREAD, y: 5.5 },
    { x: 4 - 2 * SPREAD, y: 5.5 },
    { x: 4 + 2 * SPREAD, y: 5.5 + SPREAD },
    { x: 4 - 2 * SPREAD, y: 5.5 + SPREAD },
    { x: 4 + 2 * SPREAD, y: 5.5 - SPREAD },
  ]);
});

test("at a thing too small for the dot, in front of it, past its name, inside the room", () => {
  // A living room of 4 by 4 m. A television of 1 by 0.25 m at the top wall (1.5 to 2.5 by 0.05
  // to 0.3), a shelf of 0.27 by 1.2 m at the right wall (3.7 to 3.97 by 1 to 2.2), a sofa of
  // 2 by 0.8 m (1 to 3 by 3 to 3.8). The dot (0.6 m) fits neither the television nor the shelf
  // (the fourth playtest: dots over the television and the wall): in front of each, on the side
  // toward the room's middle (2, 2), 0.05 m from it and the dot's 0.3 m further: below the
  // television at (2, 0.3 + 0.05 + 0.3), left of the shelf at (3.7 − 0.05 − 0.3, 1.6). It fits
  // the sofa: on its middle (2, 3.4).
  const living = {
    rooms: [{ id: "living", floor: 0, x: 0, y: 0, w: 4, h: 4 }],
    doors: [],
    objects: [
      { kind: "core:television", room: "living", x: 1.5, y: 0.05, w: 1, h: 0.25 },
      { kind: "core:bookshelf", room: "living", x: 3.7, y: 1, w: 0.27, h: 1.2 },
      { kind: "core:sofa", room: "living", x: 1, y: 3, w: 2, h: 0.8 },
    ],
  };
  const at = (object) => ({ place: { room: 0 }, action: { object } });
  const close = (points, expected) =>
    points.forEach((point, i) => {
      assert.ok(Math.abs(point.x - expected[i].x) < 1e-9 && Math.abs(point.y - expected[i].y) < 1e-9, `${i}: ${JSON.stringify(point)}`);
    });
  close(spots(living, [at(0), at(1), at(2)], 0), [
    { x: 2, y: 0.65 },
    { x: 3.35, y: 1.6 },
    { x: 2, y: 3.4 },
  ]);
  // With the television's name below it (its box 0.34 to 0.592 high, as `labels` puts it):
  // past the name, at 0.592 + 0.05 + 0.3; three watchers: the second to the right, the third
  // to the left.
  const names = [{ left: 1.5, top: 0.34, right: 2.6, bottom: 0.592 }, undefined, undefined];
  close(spots(living, [at(0), at(0), at(0)], 0, names), [
    { x: 2, y: 0.942 },
    { x: 2 + SPREAD, y: 0.942 },
    { x: 2 - SPREAD, y: 0.942 },
  ]);
});

test("idle where nothing around is free: the nearest free place of the room", () => {
  // A room of 2 by 2 m with a table over its top (0 to 2 by 0 to 1.4). Its middle (1, 1) and the
  // places around it lie on the table, nearer to it than the dot's 0.3 m, or past the wall (two
  // steps down, 2.3). On a grid of 0.05 m from the walls the dot keeps 0.3 m from: the nearest
  // place 0.3 m below the table, at (1, 1.7), also 0.3 m from the wall.
  const room = {
    rooms: [{ id: "small", floor: 0, x: 0, y: 0, w: 2, h: 2 }],
    doors: [],
    objects: [{ kind: "core:dining_table", room: "small", x: 0, y: 0, w: 2, h: 1.4 }],
  };
  const [p] = spots(room, [{ place: { room: 0 }, action: null }], 0);
  assert.ok(Math.abs(p.x - 1) < 1e-9 && Math.abs(p.y - 1.7) < 1e-9, JSON.stringify(p));
});

test("every dot inside its room", () => {
  // A hall of 1 by 3 m: the dot (0.6 m) fits only in its middle column. Three who do nothing
  // there: the middle (0.5, 1.5), then not to the right or left (past the wall) but below and
  // above.
  const hall = { rooms: [{ id: "hall", floor: 0, x: 0, y: 0, w: 1, h: 3 }], doors: [], objects: [] };
  const idle = { place: { room: 0 }, action: null };
  assert.deepEqual(spots(hall, [idle, idle, idle], 0), [
    { x: 0.5, y: 1.5 },
    { x: 0.5, y: 1.5 + SPREAD },
    { x: 0.5, y: 1.5 - SPREAD },
  ]);
});

test("a name below its object, the room's at the bottom of the room", async () => {
  const { labels, ROOM_SIZES, OBJECT_SIZES, textWidth } = await import("../dist/render/plan.js");
  // A room of 4 by 3 m with a table of 1 by 1 m at (1, 1). "Esstisch" at the size 0.24 is
  // (0.75 + 3 · 0.62 + 2 · 0.35 + 0.62 + 0.62) · 0.24 = 1.092 m wide (a capital, s, s, s, two
  // narrow letters t and i, c, h), wider than the table: below it from its left, 0.04 under
  // it, a line of 1.05 of the size high (0.252), the base line at 0.8 of it. "Küche" at the
  // size 0.34 is (0.75 + 4 · 0.62) · 0.34 = 1.0982 m wide and 0.357 m high, with its light edge
  // of 0.07 on either side: in the middle of the bottom, 0.05 from the wall, clear of the table
  // and its name.
  const room = { rooms: [{ id: "kitchen", floor: 0, x: 0, y: 0, w: 4, h: 3 }], doors: [], objects: [{ kind: "core:dining_table", room: "kitchen", x: 1, y: 1, w: 1, h: 1 }] };
  const { objects, rooms } = labels(room, plan(room), ["Esstisch"], ["Küche"]);
  const close = (a, b) => assert.ok(Math.abs(a - b) < 1e-9, `${a} is not ${b}`);
  assert.equal(objects[0].size, OBJECT_SIZES[0]);
  close(objects[0].x, 1);
  close(objects[0].y, 2.04 + 0.8 * 0.252);
  assert.equal(rooms[0].size, ROOM_SIZES[0]);
  close(rooms[0].x, (4 - 1.0982 - 0.14) / 2 + 0.07);
  close(textWidth("Esstisch", 0.24), 1.092);
  close(objects[0].box.right, 1 + 1.092);
  close(rooms[0].y, 3 - 0.05 - 0.357 + 0.8 * 0.357);
  assert.ok(objects[0].fits && rooms[0].fits);
});

test("in each house every name inside its room, none over another or over furniture", async () => {
  const { labels, overlap } = await import("../dist/render/plan.js");
  const { KIND_WORDS, ROOM_WORDS } = await import("../dist/text.js");
  const { readFileSync } = await import("node:fs");
  // The drawings of `data/house/houses/*.layout.json`.
  const houses = JSON.parse(readFileSync(new URL("./fixtures/layouts.json", import.meta.url), "utf8"));
  for (const [name, layout] of Object.entries(houses)) {
    const drawn = plan(layout);
    const { objects, rooms } = labels(
      layout,
      drawn,
      layout.objects.map((object) => KIND_WORDS[object.kind][0]),
      layout.rooms.map((room) => ROOM_WORDS[room.id][0]),
    );
    const boxOf = (thing) => {
      const room = layout.rooms.find((each) => each.id === (thing.room ?? thing.id));
      const left = drawn.offsets[room.floor] + thing.x;
      return { left, top: thing.y, right: left + thing.w, bottom: thing.y + thing.h };
    };
    const all = [...objects, ...rooms];
    const rooms_of = [...layout.objects.map((object) => object.room), ...layout.rooms.map((room) => room.id)];
    all.forEach((label, i) => {
      assert.ok(label.fits, `${name}: ${label.text}`);
      const room = boxOf(layout.rooms.find((each) => each.id === rooms_of[i]));
      const { box } = label;
      assert.ok(box.left >= room.left && box.right <= room.right && box.top >= room.top && box.bottom <= room.bottom, `${name}: ${label.text} past its wall`);
      all.forEach((other, j) => {
        assert.ok(i === j || overlap(box, other.box) === 0, `${name}: ${label.text} over ${other.text}`);
      });
      layout.objects.forEach((object, j) => {
        assert.ok(i === j || overlap(box, boxOf(object)) === 0, `${name}: ${label.text} over the ${object.kind}`);
      });
    });
  }
});

test("in each house a dot inside its room, in front of a thing too small for it, off the furniture and the names when idle", async () => {
  const { labels, overlap } = await import("../dist/render/plan.js");
  const { KIND_WORDS, ROOM_WORDS } = await import("../dist/text.js");
  const { readFileSync } = await import("node:fs");
  // The drawings of `data/house/houses/*.layout.json`; the dot is 0.6 m wide.
  const houses = JSON.parse(readFileSync(new URL("./fixtures/layouts.json", import.meta.url), "utf8"));
  const R = 0.3;
  for (const [name, layout] of Object.entries(houses)) {
    const drawn = plan(layout);
    const { objects: names } = labels(
      layout,
      drawn,
      layout.objects.map((object) => KIND_WORDS[object.kind][0]),
      layout.rooms.map((room) => ROOM_WORDS[room.id][0]),
    );
    const boxOf = (thing) => {
      const room = layout.rooms.find((each) => each.id === (thing.room ?? thing.id));
      const left = drawn.offsets[room.floor] + thing.x;
      return { left, top: thing.y, right: left + thing.w, bottom: thing.y + thing.h };
    };
    const dotBox = (p) => ({ left: p.x - R, top: p.y - R, right: p.x + R, bottom: p.y + R });
    const inRoom = (p, room) => {
      const box = boxOf(room);
      return p.x - R >= box.left - 1e-9 && p.x + R <= box.right + 1e-9 && p.y - R >= box.top - 1e-9 && p.y + R <= box.bottom + 1e-9;
    };
    const { rooms: roomNames } = labels(
      layout,
      drawn,
      layout.objects.map((object) => KIND_WORDS[object.kind][0]),
      layout.rooms.map((room) => ROOM_WORDS[room.id][0]),
    );
    const boxes = names.map((label) => label.box);
    const shown = [...names, ...roomNames].map((label) => label.box);
    const nameless = (p, what) =>
      shown.forEach((box) => {
        const dx = Math.max(box.left - p.x, 0, p.x - box.right);
        const dy = Math.max(box.top - p.y, 0, p.y - box.bottom);
        assert.ok(Math.hypot(dx, dy) >= R - 1e-9, `${name}: ${what} over a name`);
      });
    layout.objects.forEach((object, i) => {
      const room = layout.rooms.find((each) => each.id === object.room);
      const [p] = spots(layout, [{ place: { room: layout.rooms.indexOf(room) }, action: { object: i } }], 0, boxes, shown);
      assert.ok(inRoom(p, room), `${name}: at the ${object.kind} past its wall`);
      if (object.w < 2 * R || object.h < 2 * R) {
        assert.ok(overlap(dotBox(p), boxOf(object)) < 1e-9, `${name}: over the ${object.kind}`);
        nameless(p, `in front of the ${object.kind}`);
      }
    });
    layout.rooms.forEach((room, index) => {
      const [p] = spots(layout, [{ place: { room: index }, action: null }], 0, boxes, shown);
      assert.ok(inRoom(p, room), `${name}: idle in the ${room.id} past its wall`);
      nameless(p, `idle in the ${room.id}`);
      layout.objects.forEach((object) => {
        if (object.room === room.id) {
          const box = boxOf(object);
          const dx = Math.max(box.left - p.x, 0, p.x - box.right);
          const dy = Math.max(box.top - p.y, 0, p.y - box.bottom);
          assert.ok(Math.hypot(dx, dy) >= R - 1e-9, `${name}: idle in the ${room.id} on the ${object.kind}`);
        }
      });
    });
  }
});
