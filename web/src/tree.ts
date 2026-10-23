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
  /** Each parent and child of the hourglass, by sim, its parent by sim or by pool key. */
  readonly links: readonly (readonly [number, number])[];
};

/** How a pedigree names a person's mother and father: by sim, else by the key its pool origin
 * names (an arrival whose parents came too, a child of a pool pregnancy); `undefined` where
 * neither names a person of the pedigree (the playtest's F1). */
function parentLookup(pedigree: Pedigree): (person: Person) => [Person | undefined, Person | undefined] {
  const bySim = new Map(pedigree.persons.map((person) => [person.sim, person]));
  const byKey = new Map(pedigree.persons.map((person) => [person.lineage_key, person]));
  const one = (sim: number | null, key: string | null | undefined) =>
    sim !== null ? bySim.get(sim) : key === null || key === undefined ? undefined : byKey.get(key);
  return (person) => [one(person.mother, person.pool?.mother_key), one(person.father, person.pool?.father_key)];
}

/** The mother and the father of `person` in `pedigree`, as far as it names them. */
export function parentsOf(pedigree: Pedigree, person: Person): [Person | undefined, Person | undefined] {
  return parentLookup(pedigree)(person);
}

/** The hourglass of `sim`, or `null` if the pedigree has no such person. */
export function hourglass(pedigree: Pedigree, sim: number, up = 3, down = 2): Hourglass | null {
  const bySim = new Map(pedigree.persons.map((person) => [person.sim, person]));
  const order = new Map(pedigree.persons.map((person, index) => [person.sim, index]));
  const center = bySim.get(sim);
  if (center === undefined) {
    return null;
  }
  const lookup = parentLookup(pedigree);
  const parents = (person: Person) => lookup(person).filter((parent): parent is Person => parent !== undefined);
  const childrenOf = (person: Person) => pedigree.persons.filter((child) => parents(child).includes(person));
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
  const above = levels(up, parents);
  const below = levels(down, childrenOf);
  const drawnAbove = new Set(above.flat().map((person) => person.sim));
  // A person above and below at once (only in an impossible pedigree) stays above.
  const lower = below.map((row) => row.filter((person) => !drawnAbove.has(person.sim)));
  const drawn = [center, ...above.flat(), ...lower.flat()];
  const inGlass = new Set(drawn.map((person) => person.sim));
  const links = drawn.flatMap((child) =>
    parents(child)
      .filter((parent) => inGlass.has(parent.sim))
      .map((parent) => [parent.sim, child.sim] as const),
  );
  return { center, up: above, down: lower, links };
}
