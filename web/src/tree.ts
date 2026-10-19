import type { Pedigree, Person } from "./generated/export.js";

/**
 * The people around one person in the pedigree (P11): its biological parents up to the
 * great-grandparents, its children down to the grandchildren, each person once, in the
 * order of the pedigree. Only the pedigree's links are followed; nothing is computed.
 */
export type Hourglass = {
  readonly center: Person;
  /** `up[0]` the parents, `up[1]` the grandparents, `up[2]` the great-grandparents. */
  readonly up: readonly (readonly Person[])[];
  /** `down[0]` the children, `down[1]` the grandchildren. */
  readonly down: readonly (readonly Person[])[];
};

/** The hourglass of `sim`, or `null` if the pedigree has no such person. */
export function hourglass(pedigree: Pedigree, sim: number, up = 3, down = 2): Hourglass | null {
  const bySim = new Map(pedigree.persons.map((person) => [person.sim, person]));
  const center = bySim.get(sim);
  if (center === undefined) {
    return null;
  }
  const parentsOf = (person: Person) =>
    [person.mother, person.father].flatMap((id) => {
      const parent = id === null ? undefined : bySim.get(id);
      return parent === undefined ? [] : [parent];
    });
  const childrenOf = (person: Person) =>
    pedigree.persons.filter((child) => child.mother === person.sim || child.father === person.sim);
  // Each person is drawn once, at the first level that reaches it: in a family where
  // cousins married, a person can be a parent and a grandparent at once.
  const drawn = new Set<number>([center.sim]);
  const fresh = (person: Person) => {
    if (drawn.has(person.sim)) {
      return false;
    }
    drawn.add(person.sim);
    return true;
  };
  const levels = (count: number, next: (person: Person) => Person[]) => {
    const found: Person[][] = [];
    let level: Person[] = [center];
    for (let step = 0; step < count; step += 1) {
      level = level.flatMap(next).filter(fresh);
      found.push(level);
    }
    return found;
  };
  return { center, up: levels(up, parentsOf), down: levels(down, childrenOf) };
}
