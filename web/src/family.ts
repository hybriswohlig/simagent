import type { BodyParams } from "./generated/body.js";
import type { Pedigree, Person, Shown } from "./generated/export.js";
import { CODES, TEXT } from "./text.js";
import { parentLookup } from "./tree.js";

/** The oldest age a family picture shows: later, greying, balding and wrinkles cover what
 * the genes make alike. */
export const PICTURE_AGE = 25;

/** A relative of a person page and what it is to the person. */
export type Member = { readonly person: Person; readonly role: string };

/** One in a picture: the person, what it is to the page's person (`null` for that person),
 * its bodies at the ages it reached. */
export type Sitter = { readonly person: Person; readonly role: string | null; readonly at: readonly Shown[] };

/** A family picture: everyone in it at the same age. */
export type Picture = {
  readonly age: number;
  readonly sitters: readonly { readonly person: Person; readonly role: string | null; readonly body: BodyParams }[];
};

const byBirth = (a: Person, b: Person) => a.born - b.born || a.sim - b.sim;

/** What a relative is, as the kinship names it. */
const kin = (code: string) => CODES[`kinship ${code}`] ?? code;

/** Where a person comes from: mother and father as far as the pedigree names them, then the
 * full siblings by birth (only with both parents known). */
export function origin(pedigree: Pedigree, person: Person): Member[] {
  const parentsOf = parentLookup(pedigree);
  const [mother, father] = parentsOf(person);
  const parents: Member[] = [];
  if (mother !== undefined) {
    parents.push({ person: mother, role: TEXT.mother });
  }
  if (father !== undefined) {
    parents.push({ person: father, role: TEXT.father });
  }
  if (mother === undefined || father === undefined) {
    return parents;
  }
  const siblings = pedigree.persons
    .filter((other) => {
      if (other.sim === person.sim) {
        return false;
      }
      const [m, f] = parentsOf(other);
      return m === mother && f === father;
    })
    .sort(byBirth)
    .map((sibling) => ({
      person: sibling,
      role: kin(sibling.sex === "female" ? "sister" : "brother"),
    }));
  return [...parents, ...siblings];
}

/** A person's children by birth. */
export function children(pedigree: Pedigree, person: Person): Member[] {
  const parentsOf = parentLookup(pedigree);
  return pedigree.persons
    .filter((child) => parentsOf(child).includes(person))
    .sort(byBirth)
    .map((child) => ({ person: child, role: kin(child.sex === "female" ? "daughter" : "son") }));
}

/** The picture of `center` with `relatives` at one age: of the ages the center reached up to
 * `PICTURE_AGE`, the one at which the most relatives reached it too, the older of equal
 * counts; whoever did not reach it stays out. `null` where nobody can stand beside the center. */
export function picture(center: Sitter, relatives: readonly Sitter[]): Picture | null {
  const bodyAt = (sitter: Sitter, age: number) => sitter.at.find((shown) => shown.at.age_years === age)?.body;
  let best: { age: number; count: number } | null = null;
  for (const shown of center.at) {
    const age = shown.at.age_years;
    if (age > PICTURE_AGE) {
      continue;
    }
    const count = relatives.filter((relative) => bodyAt(relative, age) !== undefined).length;
    if (count > 0 && (best === null || count > best.count || (count === best.count && age > best.age))) {
      best = { age, count };
    }
  }
  if (best === null) {
    return null;
  }
  const age = best.age;
  return {
    age,
    sitters: [center, ...relatives].flatMap((sitter) => {
      const body = bodyAt(sitter, age);
      return body === undefined ? [] : [{ person: sitter.person, role: sitter.role, body }];
    }),
  };
}
