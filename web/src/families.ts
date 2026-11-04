import type { PeopleEntry, Pedigree, Person } from "./generated/export.js";

/** A member of a family with its age now in whole years, as the people answer gives it. */
export type Member = { readonly person: Person; readonly age: number };

/** A household of the living with its members, the oldest first. */
export type Family = { readonly household: number; readonly members: readonly Member[] };

/** The households of the living with their members and ages, by household number, for finding
 * a family to move into a house (the playtest of 2026-11-04): only those with at most `largest`
 * members (the most a house holds); with `children`, only those where a member's mother, father
 * or social parent is a member too. A person the people answer lacks (not living) is left out;
 * the members by age, the oldest first, then by sim. */
export function families(pedigree: Pedigree, people: readonly PeopleEntry[], children: boolean, largest: number): Family[] {
  const ages = new Map(people.map((entry) => [entry.sim, entry.shown.at.age_years]));
  const byHousehold = new Map<number, Member[]>();
  for (const person of pedigree.persons) {
    const age = ages.get(person.sim);
    if (age === undefined) {
      continue;
    }
    const members = byHousehold.get(person.household) ?? [];
    members.push({ person, age });
    byHousehold.set(person.household, members);
  }
  const kept: Family[] = [];
  for (const [household, members] of [...byHousehold].sort(([a], [b]) => a - b)) {
    const sims = new Set(members.map((member) => member.person.sim));
    const parented = members.some(({ person }) =>
      [person.mother, person.father, ...person.social_parents].some((parent) => parent !== null && sims.has(parent)),
    );
    if (members.length > largest || (children && !parented)) {
      continue;
    }
    members.sort((a, b) => b.age - a.age || a.person.sim - b.person.sim);
    kept.push({ household, members });
  }
  return kept;
}
