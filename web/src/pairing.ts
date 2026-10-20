import type { Pedigree, Person } from "./generated/export.js";

/** The sims in an open partnership of the pedigree (one that has not ended). */
export function openPartners(pedigree: Pedigree): Set<number> {
  const open = new Set<number>();
  for (const partnership of pedigree.partnerships) {
    if (partnership.ended === null) {
      open.add(partnership.woman);
      open.add(partnership.man);
    }
  }
  return open;
}

/** Whether a person of the pedigree lives in the town. */
function living(person: Person): boolean {
  return person.died === null && person.emigrated === null;
}

/**
 * The persons who may be offered to `sim` as a partner: if `sim` lives in the town and is in
 * no open partnership, the living of the other sex in none, in the order of the pedigree.
 * Age and kinship are not judged here: the core rejects a pair it does not allow.
 */
export function candidates(pedigree: Pedigree, sim: number): Person[] {
  const open = openPartners(pedigree);
  const person = pedigree.persons.find((p) => p.sim === sim);
  if (person === undefined || !living(person) || open.has(sim)) {
    return [];
  }
  return pedigree.persons.filter((p) => p.sex !== person.sex && living(p) && !open.has(p.sim));
}

/** The input that pairs two persons, woman and man by their sex; none for the same sex. */
export function arrangement(a: Person, b: Person): { arrange_partnership: { woman: number; man: number } } | null {
  if (a.sex === b.sex) {
    return null;
  }
  const [woman, man] = a.sex === "female" ? [a, b] : [b, a];
  return { arrange_partnership: { woman: woman.sim, man: man.sim } };
}
