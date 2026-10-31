import type { Person } from "./generated/export.js";
import type { Storage } from "./pin.js";

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

/** The input that parts two persons, woman and man by their sex; none for the same sex. */
export function separation(a: Person, b: Person): { separate_partnership: { woman: number; man: number } } | null {
  if (a.sex === b.sex) {
    return null;
  }
  const [woman, man] = a.sex === "female" ? [a, b] : [b, a];
  return { separate_partnership: { woman: woman.sim, man: man.sim } };
}
