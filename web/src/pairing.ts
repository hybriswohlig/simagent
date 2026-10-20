import type { Pedigree, Person } from "./generated/export.js";

/**
 * The persons offered to a person as partners: those the core names (`partner_candidates` of
 * the person answer, whom a pairing would be accepted with now), in its order. Age and
 * kinship are the core's to judge; a sim the pedigree lacks is left out.
 */
export function candidates(pedigree: Pedigree, offered: readonly number[]): Person[] {
  const bySim = new Map(pedigree.persons.map((person) => [person.sim, person]));
  return offered.flatMap((sim) => {
    const person = bySim.get(sim);
    return person === undefined ? [] : [person];
  });
}

/** The input that pairs two persons, woman and man by their sex; none for the same sex. */
export function arrangement(a: Person, b: Person): { arrange_partnership: { woman: number; man: number } } | null {
  if (a.sex === b.sex) {
    return null;
  }
  const [woman, man] = a.sex === "female" ? [a, b] : [b, a];
  return { arrange_partnership: { woman: woman.sim, man: man.sim } };
}

/** What pairing reports: nothing yet, a request on its way, done, or why not. */
export type Pairing =
  | { readonly state: "idle" }
  | { readonly state: "busy" }
  | { readonly state: "paired" }
  | { readonly state: "failed"; readonly text: string };

/** What the pairing part of a page shows: the form while a pair can be chosen, only the
 * note once a pair was made from it (no second request from the same page), else nothing.
 * A failure's text shows beside the form or alone. */
export function pairingShown(candidateCount: number, state: Pairing["state"]): "form" | "paired" | "nothing" {
  if (state === "paired") {
    return "paired";
  }
  return candidateCount === 0 ? "nothing" : "form";
}
