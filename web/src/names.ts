import type { Pedigree, Person } from "./generated/export.js";

/** A person's name as the export gives it (ADR-0021 section 3): first name and surname. */
export function fullName(person: Person): string {
  return `${person.first_name} ${person.surname}`;
}

/** The names of all persons of a pedigree, by sim. */
export function namesOf(pedigree: Pedigree): ReadonlyMap<number, string> {
  return new Map(pedigree.persons.map((person) => [person.sim, fullName(person)]));
}
