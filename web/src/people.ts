import type { Looks, PeopleEntry, Pedigree, Person } from "./generated/export.js";

/**
 * The filter of the person list: the sex, the age in whole years from and to (both
 * inside), single only, and per word of the looks the words kept. An empty choice keeps
 * every word; `null` keeps every sex and every age. The words come from the core (`looks` of
 * the people answer); the filter only compares.
 */
export type Filter = {
  readonly sex: Person["sex"] | null;
  readonly ageFrom: number | null;
  readonly ageTo: number | null;
  readonly singleOnly: boolean;
  readonly hair: ReadonlySet<Looks["hair"]>;
  readonly eyes: ReadonlySet<Looks["eyes"]>;
  readonly skin: ReadonlySet<Looks["skin"]>;
  readonly curls: ReadonlySet<Looks["curls"]>;
  readonly freckles: ReadonlySet<Looks["freckles"]>;
  readonly height: ReadonlySet<Looks["height"]>;
  readonly cognition: ReadonlySet<Looks["cognition"]>;
};

/** The words of the looks a filter chooses among, in the order the page offers them. */
export type Word = keyof Looks;

export const NO_FILTER: Filter = {
  sex: null,
  ageFrom: null,
  ageTo: null,
  singleOnly: false,
  hair: new Set(),
  eyes: new Set(),
  skin: new Set(),
  curls: new Set(),
  freckles: new Set(),
  height: new Set(),
  cognition: new Set(),
};

/** The set with `value` taken out if it was in, put in otherwise. */
export function toggled<T>(set: ReadonlySet<T>, value: T): Set<T> {
  const next = new Set(set);
  if (!next.delete(value)) {
    next.add(value);
  }
  return next;
}

/** The sims in a partnership of the pedigree that has not ended. */
export function partnered(pedigree: Pedigree): ReadonlySet<number> {
  const open = pedigree.partnerships.filter((p) => p.ended === null);
  return new Set(open.flatMap((p) => [p.woman, p.man]));
}

/** A person the filter kept, with its entry of the people answer. */
export type Found = { readonly person: Person; readonly entry: PeopleEntry };

function keeps<T>(choice: ReadonlySet<T>, value: T): boolean {
  return choice.size === 0 || choice.has(value);
}

/** The people `filter` keeps, in the order of the answer; a sim the pedigree lacks is left
 * out. */
export function found(pedigree: Pedigree, entries: readonly PeopleEntry[], filter: Filter): Found[] {
  const bySim = new Map(pedigree.persons.map((person) => [person.sim, person]));
  const taken = filter.singleOnly ? partnered(pedigree) : new Set<number>();
  return entries.flatMap((entry) => {
    const person = bySim.get(entry.sim);
    const age = entry.shown.at.age_years;
    const looks = entry.looks;
    const kept =
      person !== undefined &&
      (filter.sex === null || person.sex === filter.sex) &&
      (filter.ageFrom === null || age >= filter.ageFrom) &&
      (filter.ageTo === null || age <= filter.ageTo) &&
      !taken.has(entry.sim) &&
      keeps(filter.hair, looks.hair) &&
      keeps(filter.eyes, looks.eyes) &&
      keeps(filter.skin, looks.skin) &&
      keeps(filter.curls, looks.curls) &&
      keeps(filter.freckles, looks.freckles) &&
      keeps(filter.height, looks.height) &&
      keeps(filter.cognition, looks.cognition);
    return kept && person !== undefined ? [{ person, entry }] : [];
  });
}
