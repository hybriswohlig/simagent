import type { Pedigree, Person } from "./generated/export.js";

/**
 * The people around one person in the pedigree (P11): its biological parents up to the
 * great-grandparents, its children down to the grandchildren. Each person appears once, in
 * the deepest generation that reaches it within the limit (in a family where cousins
 * married, a person can be a parent and a grandparent at once; the deeper row keeps every
 * parent above its children); each row is in the order of the pedigree. Only the pedigree's
 * links are followed; social parents are not biological ones and are left out.
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
  const order = new Map(pedigree.persons.map((person, index) => [person.sim, index]));
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
  // The deepest generation of each person reached within `count` steps, then the rows.
  const levels = (count: number, next: (person: Person) => Person[]) => {
    const deepest = new Map<number, number>();
    let level: Person[] = [center];
    for (let step = 1; step <= count; step += 1) {
      level = [...new Set(level.flatMap(next))];
      for (const person of level) {
        deepest.set(person.sim, step);
      }
    }
    deepest.delete(center.sim);
    const rows: Person[][] = Array.from({ length: count }, () => []);
    for (const [id, step] of deepest) {
      const person = bySim.get(id);
      if (person !== undefined) {
        rows[step - 1]?.push(person);
      }
    }
    return rows.map((row) => row.sort((a, b) => (order.get(a.sim) ?? 0) - (order.get(b.sim) ?? 0)));
  };
  const above = levels(up, parentsOf);
  const below = levels(down, childrenOf);
  const drawnAbove = new Set(above.flat().map((person) => person.sim));
  return {
    center,
    up: above,
    // A person above and below at once (only in an impossible pedigree) stays above.
    down: below.map((row) => row.filter((person) => !drawnAbove.has(person.sim))),
  };
}
