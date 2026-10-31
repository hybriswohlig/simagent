// The family pictures of a person page on a hand-written pedigree: the origin is mother,
// father and the full siblings by birth, the children come by birth, and a picture shows its
// sitters at one age, the one of the reached ages up to 25 where the most relatives can stand
// beside the person, the older of equal counts. Every expected value is written by hand.
import { test } from "node:test";
import assert from "node:assert/strict";

import { children, origin, picture } from "../dist/family.js";

/** A person of the pedigree format with only the fields the pictures read set by hand. */
function person(sim, sex, mother, father, born) {
  return {
    sim, lineage_key: `k${sim}`, sex, alleles: [], born, died: null, cause_of_death: null,
    emigrated: null, mother, father, social_parents: [], household: 1, moved_out: false,
    carer: null, diseases: [], pool: null, first_name: `P${sim}`, surname: "S",
  };
}

// 1 and 2 are the parents of 5, 6 and 7 (7 born first); 3 is a half-sibling of 5 by 1 and 4;
// 5 and 8 are the parents of 9 and 10 (10 born first); 11 came from the pool as a child of 8,
// named by its key only.
const PEDIGREE = {
  format_version: 5,
  persons: [
    person(1, "female", null, null, 0),
    person(2, "male", null, null, 0),
    person(3, "female", 1, 4, 50),
    person(4, "male", null, null, 0),
    person(5, "female", 1, 2, 300),
    person(6, "male", 1, 2, 400),
    person(7, "female", 1, 2, 200),
    person(8, "male", null, null, 0),
    person(9, "male", 5, 8, 900),
    person(10, "female", 5, 8, 800),
    { ...person(11, "female", null, null, 700), pool: { mother_key: null, father_key: "k8" } },
  ],
  partnerships: [],
  households: [],
};

const sims = (members) => members.map((member) => [member.person.sim, member.role]);

test("the origin: mother, father and the full siblings by birth", () => {
  const five = PEDIGREE.persons[4];
  assert.deepEqual(sims(origin(PEDIGREE, five)), [
    [1, "Mutter"],
    [2, "Vater"],
    [7, "Schwester"],
    [6, "Bruder"],
  ]);
});

test("without both parents there is no sibling, and an unknown parent is left out", () => {
  assert.deepEqual(sims(origin(PEDIGREE, PEDIGREE.persons[0])), []);
  assert.deepEqual(sims(origin(PEDIGREE, PEDIGREE.persons[10])), [[8, "Vater"]]);
});

test("the children by birth, also one its pool origin names", () => {
  assert.deepEqual(sims(children(PEDIGREE, PEDIGREE.persons[4])), [
    [10, "Tochter"],
    [9, "Sohn"],
  ]);
  assert.deepEqual(sims(children(PEDIGREE, PEDIGREE.persons[7])), [
    [11, "Tochter"],
    [10, "Tochter"],
    [9, "Sohn"],
  ]);
  assert.deepEqual(sims(children(PEDIGREE, PEDIGREE.persons[5])), []);
});

/** Bodies at the ages a sitter reached; the body names its sim and age, to be told apart. */
function at(sim, ...ages) {
  return ages.map((age) => ({ at: { t: 0, age_years: age, calendar_year: 0 }, body: { sim, age } }));
}
const sitter = (sim, role, ...ages) => ({ person: PEDIGREE.persons[sim - 1], role, at: at(sim, ...ages) });

test("a picture at the reached age up to 25 where the most can stand beside the person", () => {
  const center = sitter(5, null, 2, 8, 12, 16, 25, 40);
  const shown = picture(center, [
    sitter(1, "Mutter", 2, 8, 12, 16, 25, 40, 60),
    sitter(2, "Vater", 2, 8, 12, 16, 25, 40, 60),
    sitter(7, "Schwester", 2, 8, 12, 16, 25, 40),
    sitter(6, "Bruder", 2, 8, 12),
  ]);
  assert.equal(shown.age, 12);
  assert.deepEqual(
    shown.sitters.map((each) => [each.person.sim, each.role, each.body]),
    [
      [5, null, { sim: 5, age: 12 }],
      [1, "Mutter", { sim: 1, age: 12 }],
      [2, "Vater", { sim: 2, age: 12 }],
      [7, "Schwester", { sim: 7, age: 12 }],
      [6, "Bruder", { sim: 6, age: 12 }],
    ],
  );
});

test("of equal counts the older age, never above 25, and whoever did not reach it stays out", () => {
  const center = sitter(5, null, 2, 8, 12, 16, 25, 40);
  const shown = picture(center, [sitter(1, "Mutter", 2, 8, 12, 16, 25, 40, 60), sitter(6, "Bruder", 2, 8)]);
  assert.equal(shown.age, 8);
  const older = picture(center, [sitter(1, "Mutter", 2, 8, 12, 16, 25, 40, 60), sitter(2, "Vater", 2, 8, 12, 16, 25, 40, 60)]);
  assert.equal(older.age, 25);
  assert.deepEqual(older.sitters.map((each) => each.person.sim), [5, 1, 2]);
  const young = picture(sitter(5, null, 2, 8, 12), [sitter(10, "Tochter", 2, 8), sitter(9, "Sohn", 2)]);
  assert.equal(young.age, 2);
  assert.deepEqual(young.sitters.map((each) => each.person.sim), [5, 10, 9]);
  const one = picture(sitter(5, null, 2, 8, 12), [sitter(10, "Tochter", 2, 8), sitter(9, "Sohn")]);
  assert.equal(one.age, 8);
  assert.deepEqual(one.sitters.map((each) => each.person.sim), [5, 10]);
});

test("no picture where nobody stands beside the person or the person has no reached age", () => {
  assert.equal(picture(sitter(5, null, 2, 8), []), null);
  assert.equal(picture(sitter(5, null, 2, 8), [sitter(9, "Sohn")]), null);
  assert.equal(picture(sitter(5, null), [sitter(1, "Mutter", 2, 8)]), null);
  assert.equal(picture(sitter(5, null, 40, 60), [sitter(1, "Mutter", 40, 60)]), null);
});
