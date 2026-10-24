import type { Age, Pedigree, Person } from "./generated/export.js";
import { fullName } from "./names.js";
import { TEXT } from "./text.js";

/** The ages of the living by sim, as the core gives them: whole years at the last closed
 * second (Grok's round 33, P3). */
export function agesOf(ages: readonly Age[]): ReadonlyMap<number, number> {
  return new Map(ages.map((age) => [age.sim, age.age_years]));
}

const BASE: Readonly<Record<string, string>> = { Ä: "A", Ö: "O", Ü: "U", ẞ: "S", ä: "a", ö: "o", ü: "u", ß: "s" };

/** The key a name sorts by (DIN 5007-1, Grok's round 33, P4): Ä, Ö, Ü and ß as A, O, U and S;
 * the name shown keeps its letters. */
export function nameKey(name: string): string {
  return name.replace(/[ÄÖÜẞäöüß]/g, (letter) => BASE[letter] ?? letter);
}

/** A person offered as a partner: the name and the age (Grok's round 33, P3). */
export function offerLabel(person: Person, ages: ReadonlyMap<number, number>): string {
  const age = ages.get(person.sim);
  return age === undefined ? fullName(person) : `${fullName(person)}, ${age}`;
}

/** The persons to ask the kinship with (Grok's round 33, P4), without `self`: first who lives
 * in the town, then everyone else of the pedigree, each group by name (`nameKey`, then the
 * name itself, then the smaller sim); the living with their age, the dead with "†", who
 * moved away with "weggezogen". */
export function kinChoices(
  pedigree: Pedigree,
  ages: ReadonlyMap<number, number>,
  self: number,
): { sim: number; label: string }[] {
  const byName = (a: Person, b: Person) =>
    compare(nameKey(fullName(a)), nameKey(fullName(b))) || compare(fullName(a), fullName(b)) || a.sim - b.sim;
  const others = pedigree.persons.filter((person) => person.sim !== self);
  const living = others.filter((person) => person.died === null && person.emigrated === null).sort(byName);
  const gone = others.filter((person) => person.died !== null || person.emigrated !== null).sort(byName);
  return [
    ...living.map((person) => ({ sim: person.sim, label: offerLabel(person, ages) })),
    ...gone.map((person) => ({
      sim: person.sim,
      label: `${fullName(person)}, ${person.died === null ? TEXT.tree_away : TEXT.tree_dead_mark}`,
    })),
  ];
}

/** The order of two texts by their code units, the same in every browser. */
function compare(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}
