import type { CalendarDate, Person } from "./generated/export.js";
import type { Storage } from "./pin.js";
import { TEXT } from "./text.js";

const KEY = "simagent.cheat";

/**
 * The cheat mode (ADR-0023, addendum of 2026-10-31): a switch of the viewer, off at first and
 * kept in the browser's storage, no state of a run. A storage that is missing or throws keeps
 * it off and fails nothing.
 */
export function cheatOn(storage: Storage | null): boolean {
  try {
    return storage?.getItem(KEY) === "on";
  } catch {
    return false;
  }
}

export function setCheat(storage: Storage | null, on: boolean): void {
  try {
    storage?.setItem(KEY, on ? "on" : "off");
  } catch {
    // The switch is not kept.
  }
}

/** Two persons as woman and man by their sex; none for the same sex. */
function couple(a: Person, b: Person): { woman: number; man: number } | null {
  if (a.sex === b.sex) {
    return null;
  }
  const [woman, man] = a.sex === "female" ? [a, b] : [b, a];
  return { woman: woman.sim, man: man.sim };
}

/** The input that parts two persons, woman and man by their sex; none for the same sex. */
export function separation(a: Person, b: Person): { separate_partnership: { woman: number; man: number } } | null {
  const pair = couple(a, b);
  return pair === null ? null : { separate_partnership: pair };
}

/** The input of "Zeit zu zweit" for two persons (ADR-0023, addendum of 2026-11-01), woman and
 * man by their sex; none for the same sex. */
export function timeTogether(a: Person, b: Person): { time_together: { woman: number; man: number } } | null {
  const pair = couple(a, b);
  return pair === null ? null : { time_together: pair };
}

/** The line of a window of "Zeit zu zweit" until its last month start, as the core dates it
 * (W5): its year and its month, counted from 1. */
export function togetherUntil(date: CalendarDate): string {
  return TEXT.together_until.replace("{year}", String(date.year)).replace("{month}", String(date.month + 1));
}
