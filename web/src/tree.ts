import type { Pedigree, Person } from "./generated/export.js";
import { fullName } from "./names.js";
import { TEXT } from "./text.js";

/** How the tree marks a person who died or moved away (Grok round 31, T1 and T2): short in
 * its node, in full for a screen reader; nothing for the living. */
export function nodeMark(person: Person): { shown: string | null; spoken: string } {
  const name = fullName(person);
  if (person.died !== null) {
    return { shown: TEXT.tree_dead_mark, spoken: `${name}, ${TEXT.tree_dead}` };
  }
  if (person.emigrated !== null) {
    return { shown: TEXT.tree_away, spoken: `${name}, ${TEXT.tree_away}` };
  }
  return { shown: null, spoken: name };
}

/** The partner of `person` in a partnership that has not ended, if any (Grok T3). */
export function currentPartner(pedigree: Pedigree, person: Person): Person | undefined {
  const bond = pedigree.partnerships.find(
    (couple) => couple.ended === null && (couple.woman === person.sim || couple.man === person.sim),
  );
  const other = bond === undefined ? undefined : bond.woman === person.sim ? bond.man : bond.woman;
  return other === undefined ? undefined : pedigree.persons.find((each) => each.sim === other);
}

/**
 * The people around one person in the pedigree (P11): its biological parents up to the
 * great-grandparents, its children down to the grandchildren. Each person appears once, in
 * the deepest generation that reaches it within the limit (in a family where cousins
 * married, a person can be a parent and a grandparent at once; the deeper row keeps every
 * parent above its children). Each row stands in the order of its people's relatives in the
 * row nearer the person, children under their parents and ancestors above their children, so
 * that lines do not cross for the order of birth alone (the fourth playtest); among people
 * of the same place in the order of the pedigree. Only the pedigree's links are followed;
 * social parents are not biological ones and are left out.
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
  // The deepest generation of each person reached within `count` steps, then the rows, each
  // in the order of the places its people's relatives (`back`) take in the row before it.
  const levels = (count: number, next: (person: Person) => Person[], back: (person: Person) => Person[]) => {
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
    let before: readonly Person[] = [center];
    return rows.map((row) => {
      const place = new Map(before.map((person, at) => [person.sim, at]));
      const first = (person: Person) =>
        back(person).reduce((low, relative) => {
          const at = place.get(relative.sim);
          return at !== undefined && at < low ? at : low;
        }, Number.POSITIVE_INFINITY);
      const sorted = row
        .map((person) => ({ person, first: first(person), order: order.get(person.sim) ?? 0 }))
        .sort((a, b) => (a.first === b.first ? a.order - b.order : a.first < b.first ? -1 : 1))
        .map((entry) => entry.person);
      before = sorted;
      return sorted;
    });
  };
  const above = levels(up, parents, childrenOf);
  const below = levels(down, childrenOf, parents);
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
