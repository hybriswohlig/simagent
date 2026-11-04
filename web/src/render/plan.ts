import type { HouseLayout, HousePerson } from "../generated/export.js";

/** The gap between two floors drawn side by side, in metres. */
export const FLOOR_GAP = 1.5;
/** How far apart people drawn at one spot stand, in metres. */
export const SPREAD = 0.45;

export type Point = { readonly x: number; readonly y: number };

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

/** Where each person is drawn at the second `at`, only for the picture (ADR-0025 section 5):
 * at its object while it does something there, in the middle of its room while it does
 * nothing, on the way along the straight line between the middles of the two rooms by the
 * share of the walk gone. Several at one spot stand side by side. */
export function spots(layout: HouseLayout, persons: readonly HousePerson[], at: number): Point[] {
  const drawn = plan(layout);
  const placed = persons.map((person): Point => {
    const { room, walking } = person.place;
    if (walking !== undefined) {
      const from = roomMiddle(layout, drawn, walking.from);
      const to = roomMiddle(layout, drawn, walking.to);
      const length = walking.arrive.t - walking.depart.t;
      const share = length <= 0 ? 1 : Math.min(1, Math.max(0, (at - walking.depart.t) / length));
      return { x: from.x + (to.x - from.x) * share, y: from.y + (to.y - from.y) * share };
    }
    if (person.action !== null) {
      return objectMiddle(layout, drawn, person.action.object);
    }
    return roomMiddle(layout, drawn, room ?? 0);
  });
  // The second at a spot stands to the right of the first, and so on.
  return placed.map((point, index) => {
    const before = placed.slice(0, index).filter((other) => other.x === point.x && other.y === point.y);
    return { x: point.x + SPREAD * before.length, y: point.y };
  });
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
