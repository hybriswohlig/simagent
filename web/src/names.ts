import type { Pedigree, Person } from "./generated/export.js";

/** A person's name as the export gives it (ADR-0021 section 3): first name and surname. */
export function fullName(person: Person): string {
  return `${person.first_name} ${person.surname}`;
}

/** A person of one run as a comparison names it: its name and its sim there. */
export type Named = { name: string; sim: number };

/** The names and sims of all persons of a pedigree, by lineage key (ADR-0021 section 3: a
 * comparison goes by key, each run named from its own pedigree). */
export function namesByKey(pedigree: Pedigree): ReadonlyMap<string, Named> {
  return new Map(pedigree.persons.map((person) => [person.lineage_key, { name: fullName(person), sim: person.sim }]));
}

/** The names of all persons of a pedigree, by sim. */
export function namesOf(pedigree: Pedigree): ReadonlyMap<number, string> {
  return new Map(pedigree.persons.map((person) => [person.sim, fullName(person)]));
}
