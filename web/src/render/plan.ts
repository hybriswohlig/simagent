import type { HouseLayout, HouseWalk } from "../generated/export.js";

/** The gap between two floors drawn side by side, in metres. */
export const FLOOR_GAP = 1.5;
/** How far apart the middles of two people drawn stand at least, in metres: a dot's width
 * (0.6) and a little. */
export const SPREAD = 0.65;

export type Point = { readonly x: number; readonly y: number };

/** Someone drawn in the plan: a person with needs, or a child under 6, who has no action. */
export type Drawn = {
  readonly place: { readonly room?: number; readonly walking?: HouseWalk };
  readonly action?: { readonly object: number } | null;
};

/** The floors of a drawing side by side, from the ground floor: where each begins on the
 * right, and the size of the whole, in metres (ADR-0025 section 1). */
export type Plan = {
  readonly offsets: readonly number[];
  readonly width: number;
  readonly height: number;
};

export function plan(layout: HouseLayout): Plan {
  const floors = 1 + Math.max(0, ...layout.rooms.map((room) => room.floor));
  const offsets: number[] = [];
  let left = 0;
  let height = 0;
  for (let floor = 0; floor < floors; floor += 1) {
    const rooms = layout.rooms.filter((room) => room.floor === floor);
    offsets.push(left);
    left += Math.max(0, ...rooms.map((room) => room.x + room.w)) + FLOOR_GAP;
    height = Math.max(height, ...rooms.map((room) => room.y + room.h));
  }
  return { offsets, width: Math.max(0, left - FLOOR_GAP), height };
}

/** The middle of room `index` in the plan. */
export function roomMiddle(layout: HouseLayout, drawn: Plan, index: number): Point {
  const room = layout.rooms[index];
  if (room === undefined) {
    return { x: 0, y: 0 };
  }
  return { x: (drawn.offsets[room.floor] ?? 0) + room.x + room.w / 2, y: room.y + room.h / 2 };
}

/** The middle of object `index` in the plan. */
export function objectMiddle(layout: HouseLayout, drawn: Plan, index: number): Point {
  const object = layout.objects[index];
  const room = layout.rooms.find((room) => room.id === object?.room);
  if (object === undefined || room === undefined) {
    return { x: 0, y: 0 };
  }
  const offset = drawn.offsets[room.floor] ?? 0;
  return { x: offset + object.x + object.w / 2, y: object.y + object.h / 2 };
}

/** The radius of a person's dot, in metres, and how far it stands from a thing it is too large
 * for. */
const DOT = 0.3;
const FRONT = 0.05;
/** The step of the places in a room tried for one who does nothing where none around its place
 * is free, in metres. */
const GRID = 0.05;

/** The places a dot can stand in `room`, on a grid of `GRID`, the nearest to `point` first, then
 * from the top and the left. */
function grid(room: Box, point: Point): Point[] {
  const count = (span: number) => Math.max(0, Math.floor((span - 2 * DOT) / GRID + 1e-9)) + 1;
  const places: Point[] = [];
  for (let row = 0; row < count(room.bottom - room.top); row += 1) {
    for (let column = 0; column < count(room.right - room.left); column += 1) {
      places.push({ x: room.left + DOT + column * GRID, y: room.top + DOT + row * GRID });
    }
  }
  const away = (spot: Point) => Math.hypot(spot.x - point.x, spot.y - point.y);
  return places.sort((a, b) => away(a) - away(b) || a.y - b.y || a.x - b.x);
}

/** Whether a dot at `point` lies inside `room`. */
function within(point: Point, room: Box): boolean {
  return point.x - DOT >= room.left - 1e-9 && point.x + DOT <= room.right + 1e-9 && point.y - DOT >= room.top - 1e-9 && point.y + DOT <= room.bottom + 1e-9;
}

/** Whether a dot at `point` keeps clear of `box` (its nearest point at least the dot's radius
 * away). */
function clear(point: Point, box: Box): boolean {
  const dx = Math.max(box.left - point.x, 0, point.x - box.right);
  const dy = Math.max(box.top - point.y, 0, point.y - box.bottom);
  return Math.hypot(dx, dy) >= DOT - 1e-9;
}

/** Where someone at object `index` stands: on its middle where the dot fits on it (a bed, the
 * sofa, the table), else in front of it (the fourth playtest: dots over the television and the
 * wall): on the side toward the middle of its room, across the thin side (the height first),
 * `FRONT` from it or, where its name `name` stands on that side, from its name, and the dot's
 * radius further. */
function atObject(layout: HouseLayout, drawn: Plan, index: number, name: Box | undefined): Point {
  const middle = objectMiddle(layout, drawn, index);
  const box = objectBox(layout, drawn, index);
  const roomIndex = layout.rooms.findIndex((room) => room.id === layout.objects[index]?.room);
  const centre = roomMiddle(layout, drawn, roomIndex);
  const thinY = box.bottom - box.top < 2 * DOT;
  const thinX = box.right - box.left < 2 * DOT;
  if (!thinY && !thinX) {
    return middle;
  }
  // The name counts where it stands across the dot's way out on that side.
  const named = name === undefined ? null : name;
  if (thinY) {
    const across = named !== null && named.left < middle.x + DOT && named.right > middle.x - DOT;
    if (centre.y >= middle.y) {
      const past = across && named.top >= box.top ? named.bottom : box.bottom;
      return { x: middle.x, y: Math.max(box.bottom, past) + FRONT + DOT };
    }
    const past = across && named.bottom <= box.bottom ? named.top : box.top;
    return { x: middle.x, y: Math.min(box.top, past) - FRONT - DOT };
  }
  const across = named !== null && named.top < middle.y + DOT && named.bottom > middle.y - DOT;
  if (centre.x >= middle.x) {
    const past = across && named.left >= box.left ? named.right : box.right;
    return { x: Math.max(box.right, past) + FRONT + DOT, y: middle.y };
  }
  const past = across && named.right <= box.right ? named.left : box.left;
  return { x: Math.min(box.left, past) - FRONT - DOT, y: middle.y };
}

/** Where each person is drawn at the second `at`, only for the picture (ADR-0025 section 5):
 * at its object while it does something there (`atObject`, with the object names `names`
 * where `labels` put them), in the middle of its room while it does nothing (a child always),
 * on the way along the straight line between the middles of the two rooms by the share of the
 * walk gone. Nobody covers another (the playtest of 2026-11-04): one who would stand nearer
 * than `SPREAD` to one drawn before stands at the first place around its own that is not, to
 * the right, the left, below, above, then at the corners, then twice as far. One in a room
 * stays inside it, and one who does nothing or stands in front of a thing stays off the
 * furniture and off every name in `shown` (the fourth playtest), where no place around is so at
 * the nearest such place of the room (`grid`); failing that, the same without the names; where
 * none is so, the first place around that covers nobody. */
export function spots(
  layout: HouseLayout,
  persons: readonly Drawn[],
  at: number,
  names: readonly (Box | undefined)[] = [],
  shown: readonly Box[] = [],
): Point[] {
  const drawn = plan(layout);
  const things = layout.objects.map((_, index) => ({ room: layout.objects[index]?.room, box: objectBox(layout, drawn, index) }));
  const placed = persons.map((person): { point: Point; room: number | null; floor: boolean } => {
    const { room, walking } = person.place;
    if (walking !== undefined) {
      const from = roomMiddle(layout, drawn, walking.from);
      const to = roomMiddle(layout, drawn, walking.to);
      const length = walking.arrive.t - walking.depart.t;
      const share = length <= 0 ? 1 : Math.min(1, Math.max(0, (at - walking.depart.t) / length));
      return { point: { x: from.x + (to.x - from.x) * share, y: from.y + (to.y - from.y) * share }, room: null, floor: false };
    }
    if (person.action !== undefined && person.action !== null) {
      const object = person.action.object;
      const roomIndex = layout.rooms.findIndex((each) => each.id === layout.objects[object]?.room);
      const point = atObject(layout, drawn, object, names[object]);
      // In front of a thing, not on it: on the floor.
      const floor = point.x !== objectMiddle(layout, drawn, object).x || point.y !== objectMiddle(layout, drawn, object).y;
      return { point, room: roomIndex < 0 ? null : roomIndex, floor };
    }
    return { point: roomMiddle(layout, drawn, room ?? 0), room: room ?? 0, floor: true };
  });
  const ring = [
    [0, 0], [1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [-1, 1], [1, -1], [-1, -1],
    [2, 0], [-2, 0], [0, 2], [0, -2], [2, 1], [-2, 1], [2, -1], [-2, -1],
    [1, 2], [-1, 2], [1, -2], [-1, -2], [2, 2], [-2, 2], [2, -2], [-2, -2],
  ] as const;
  const out: Point[] = [];
  for (const { point, room, floor } of placed) {
    const free = (spot: Point) => out.every((other) => Math.hypot(other.x - spot.x, other.y - spot.y) >= SPREAD - 1e-9);
    const box = room === null ? null : roomBox(layout, drawn, room);
    const id = room === null ? null : layout.rooms[room]?.id;
    const fits = (spot: Point) =>
      box === null || (within(spot, box) && (!floor || things.every((thing) => thing.room !== id || clear(spot, thing.box))));
    const unnamed = (spot: Point) => shown.every((name) => clear(spot, name));
    const around = ring.map(([dx, dy]) => ({ x: point.x + dx * SPREAD, y: point.y + dy * SPREAD }));
    const roomy = (spot: Point) => free(spot) && fits(spot);
    const best = (spot: Point) => roomy(spot) && unnamed(spot);
    const elsewhere = (good: (spot: Point) => boolean) => (floor && box !== null ? grid(box, point).find(good) : undefined);
    out.push(
      around.find(best) ??
        elsewhere(best) ??
        around.find(roomy) ??
        elsewhere(roomy) ??
        around.find(free) ??
        around[around.length - 1] ??
        point,
    );
  }
  return out;
}

/** The sizes of the names in the plan, in metres: of a room, then smaller where it does not
 * fit; of an object likewise. */
export const ROOM_SIZES = [0.34, 0.28, 0.24, 0.2] as const;
export const OBJECT_SIZES = [0.24, 0.2, 0.17] as const;
/** The height of a line of a name as a share of its size. */
const LINE = 1.05;

/** How wide `text` is drawn at `size`, in metres: per letter a share of the size wide enough for
 * a broad sans-serif face, narrow letters 0.35, m and w 0.95, other capitals 0.75, the rest
 * 0.62. */
export function textWidth(text: string, size: number): number {
  let width = 0;
  for (const letter of text) {
    width += "iljtfrI.,:;'!| ".includes(letter) ? 0.35 : "mwMW".includes(letter) ? 0.95 : letter !== letter.toLowerCase() ? 0.75 : 0.62;
  }
  return width * size;
}
/** The room a name keeps from the walls, and the gap between an object and its name. */
const MARGIN = 0.05;
const GAP = 0.04;
/** The light edge around a room's name (its stroke of 0.12), kept beside it in its box; above
 * and below the line has room for it. */
const HALO = 0.07;

/** A box in the plan: left, top, right, bottom in metres. */
export type Box = { readonly left: number; readonly top: number; readonly right: number; readonly bottom: number };

/** A name in the plan: its text, where its line begins (`x`) and its base line (`y`), its size,
 * its box, and whether it found a place inside its room clear of every other name. */
export type Label = {
  readonly text: string;
  readonly x: number;
  readonly y: number;
  readonly size: number;
  readonly box: Box;
  readonly fits: boolean;
};

/** How much two boxes overlap, in square metres. */
export function overlap(a: Box, b: Box): number {
  const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
  const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
  return w > 0 && h > 0 ? w * h : 0;
}

/** Whether `inner` lies inside `outer`. */
function inside(inner: Box, outer: Box): boolean {
  return inner.left >= outer.left && inner.right <= outer.right && inner.top >= outer.top && inner.bottom <= outer.bottom;
}

/** The box of a room in the plan, or of an object. */
function roomBox(layout: HouseLayout, drawn: Plan, index: number): Box {
  const room = layout.rooms[index];
  const left = (drawn.offsets[room?.floor ?? 0] ?? 0) + (room?.x ?? 0);
  return { left, top: room?.y ?? 0, right: left + (room?.w ?? 0), bottom: (room?.y ?? 0) + (room?.h ?? 0) };
}

function objectBox(layout: HouseLayout, drawn: Plan, index: number): Box {
  const object = layout.objects[index];
  const room = layout.rooms.find((each) => each.id === object?.room);
  const left = (drawn.offsets[room?.floor ?? 0] ?? 0) + (object?.x ?? 0);
  return { left, top: object?.y ?? 0, right: left + (object?.w ?? 0), bottom: (object?.y ?? 0) + (object?.h ?? 0) };
}

/** The places a name of `size` may stand in turn, as the left and top of its box, and whether
 * it may stand on its own object (only inside it). */
type Places = (width: number, height: number) => readonly (readonly [number, number, boolean?])[];

/** The first place of `places` for `text`, from the largest of `sizes` on, each as given and
 * then moved sideways inside the room, whose box (with `pad` beside the text) lies inside
 * `room`, clear of every box in `taken` and of every object box in `objects` (`own`, the name's
 * object, unless the place is inside it); failing that, the place clear of `taken` inside
 * `room` that covers the least of the objects; failing that, the first place of the smallest
 * size, which does not fit. */
function place(
  text: string,
  sizes: readonly number[],
  places: Places,
  room: Box,
  { objects, own, pad }: { readonly objects: readonly Box[]; readonly own: Box | null; readonly pad: number },
  taken: readonly Box[],
): Label {
  const inner = { left: room.left + MARGIN, top: room.top + MARGIN, right: room.right - MARGIN, bottom: room.bottom - MARGIN };
  let best: { label: Label; covers: number } | null = null;
  for (const size of sizes) {
    const width = textWidth(text, size) + 2 * pad;
    const height = LINE * size;
    const tried = places(width, height).flatMap(([left, top, onOwn]) => {
      const moved = Math.min(Math.max(left, inner.left), inner.right - width);
      return moved === left ? [[left, top, onOwn] as const] : [[left, top, onOwn] as const, [moved, top, onOwn] as const];
    });
    for (const [left, top, onOwn] of tried) {
      const box = { left, top, right: left + width, bottom: top + height };
      // The base line sits a fifth of the line above the bottom of the text.
      const label = { text, x: left + pad, y: top + 0.8 * height, size, box, fits: true };
      if (!inside(box, inner) || taken.some((other) => overlap(box, other) > 0)) {
        continue;
      }
      const mine = own === null || onOwn === true ? 0 : overlap(box, own);
      const covers = objects.reduce((sum, object) => sum + overlap(box, object), mine);
      if (covers === 0) {
        return label;
      }
      if (best === null || covers < best.covers) {
        best = { label, covers };
      }
    }
  }
  if (best !== null) {
    return best.label;
  }
  const smallest = sizes[sizes.length - 1] ?? 0;
  const [width, height] = [textWidth(text, smallest) + 2 * pad, LINE * smallest];
  const [left, top] = places(width, height)[0] ?? [room.left, room.top];
  const box = { left, top, right: left + width, bottom: top + height };
  return { text, x: left + pad, y: top + 0.8 * height, size: smallest, box, fits: false };
}

/** The names of the plan (the playtest of 2026-11-04: no name over another or past its wall):
 * first each object's, `objectNames[i]` for object i, inside it if it fits, else beside it,
 * below, above, right, left, then a line further below or above; then each room's,
 * `roomNames[i]` for room i, at the bottom, the top or the middle of the room, else from the
 * top down a tenth of a metre at a time, each in the middle, left or right. Each stands inside its room clear of the names before it and, where it can,
 * of the objects, in a smaller size where the larger finds no place; a room's name with its
 * light edge. */
export function labels(layout: HouseLayout, drawn: Plan, objectNames: readonly string[], roomNames: readonly string[]): { objects: Label[]; rooms: Label[] } {
  const taken: Box[] = [];
  const things = layout.objects.map((_, index) => objectBox(layout, drawn, index));
  const objects = layout.objects.map((object, index) => {
    const own = things[index] ?? { left: 0, top: 0, right: 0, bottom: 0 };
    const roomIndex = layout.rooms.findIndex((room) => room.id === object.room);
    const room = roomBox(layout, drawn, roomIndex);
    const places: Places = (width, height) => {
      const lefts = [own.left, own.right - width, (own.left + own.right - width) / 2];
      const below = (row: number) => lefts.map((left) => [left, own.bottom + GAP + row * height] as const);
      const above = (row: number) => lefts.map((left) => [left, own.top - GAP - (row + 1) * height] as const);
      const middle = (own.top + own.bottom - height) / 2;
      const within: (readonly [number, number, boolean])[] =
        width <= own.right - own.left - 2 * GAP && height <= own.bottom - own.top - 2 * GAP
          ? [[(own.left + own.right - width) / 2, middle, true]]
          : [];
      return [
        ...within,
        ...below(0),
        ...above(0),
        [own.right + GAP, middle],
        [own.left - GAP - width, middle],
        ...below(1),
        ...above(1),
        ...below(2),
        ...above(2),
      ];
    };
    const others = things.filter((_, other) => other !== index);
    const label = place(objectNames[index] ?? "", OBJECT_SIZES, places, room, { objects: others, own, pad: 0 }, taken);
    taken.push(label.box);
    return label;
  });
  const rooms = layout.rooms.map((room, index) => {
    const box = roomBox(layout, drawn, index);
    const places: Places = (width, height) => {
      const lefts = [(box.left + box.right - width) / 2, box.left + MARGIN, box.right - MARGIN - width];
      const tops = [box.bottom - MARGIN - height, box.top + MARGIN, (box.top + box.bottom - height) / 2];
      // Then from the top down in steps of a tenth of a metre.
      const steps = Math.max(0, Math.floor((box.bottom - box.top - 2 * MARGIN - height) / 0.1));
      const down = Array.from({ length: steps + 1 }, (_, step) => box.top + MARGIN + step * 0.1);
      return [...tops, ...down].flatMap((top) => lefts.map((left) => [left, top] as const));
    };
    const within = layout.objects.flatMap((object, i) => (object.room === room.id ? [things[i] ?? box] : []));
    const label = place(roomNames[index] ?? "", ROOM_SIZES, places, box, { objects: within, own: null, pad: HALO }, taken);
    taken.push(label.box);
    return label;
  });
  return { objects, rooms };
}

/** The whole minutes of a span of seconds, as a delay reads: 1 295 s are 21 minutes. */
export function wholeMinutes(seconds: number): number {
  return Math.floor(seconds / 60);
}

/** A second of the day as a clock reads it, "6:05". */
export function timeOfDay(secondOfDay: number): string {
  const minutes = Math.floor(secondOfDay / 60);
  const hours = Math.floor(minutes / 60);
  return `${hours}:${String(minutes % 60).padStart(2, "0")}`;
}
