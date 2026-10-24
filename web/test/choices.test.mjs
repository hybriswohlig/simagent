// The lists a person page chooses from (Grok's round 33, P3 and P4): a partner offered with
// the name and the age in whole years the core gives; the kinship list first who lives in
// the town, then everyone else of the pedigree, each group by name (DIN 5007-1: Ä, Ö, Ü and
// ß as A, O, U and S; then the name itself, then the smaller sim), the living with their
// age, the dead with "†", who moved away with "weggezogen". The orders are written by hand
// from those rules.
import { test } from "node:test";
import assert from "node:assert/strict";

import { agesOf, kinChoices, nameKey, offerLabel } from "../dist/choices.js";

/** A person of a pedigree; the values follow the format, they are not computed. */
function person(sim, first_name, surname, ended = {}) {
  return {
    sim,
    lineage_key: "00000000000000aa",
    sex: "female",
    alleles: [],
    born: -1,
    died: null,
    cause_of_death: null,
    emigrated: null,
    mother: null,
    father: null,
    social_parents: [],
    household: 1,
    moved_out: false,
    carer: null,
    diseases: [],
    pool: null,
    first_name,
    surname,
    ...ended,
  };
}

const PEDIGREE = {
  format_version: 4,
  persons: [
    person(1, "Otto", "Ast"),
    person(2, "Ömer", "Bauer"),
    person(3, "Olaf", "Christ"),
    person(4, "Anna", "Zeh", { died: 100, cause_of_death: "age" }),
    person(5, "Bert", "Yu", { emigrated: 200 }),
    person(6, "Möller", "Kai"),
    person(7, "Moller", "Kai"),
    person(8, "Moller", "Kai"),
    person(9, "Ida", "Self"),
    person(10, "Ärne", "Vogt", { died: 300, cause_of_death: "accident" }),
  ],
  partnerships: [],
};

const AGES = agesOf([
  { sim: 1, age_years: 105 },
  { sim: 2, age_years: 21 },
  { sim: 3, age_years: 40 },
  { sim: 6, age_years: 30 },
  { sim: 7, age_years: 31 },
  { sim: 8, age_years: 32 },
  { sim: 9, age_years: 21 },
]);

test("a partner offered with the age the core gives", () => {
  assert.equal(offerLabel(PEDIGREE.persons[0], AGES), "Otto Ast, 105");
  assert.equal(offerLabel(PEDIGREE.persons[1], AGES), "Ömer Bauer, 21");
});

test("the name key reads umlauts and ß as their base letters and keeps the rest", () => {
  assert.equal(nameKey("Ömer Bauer"), "Omer Bauer");
  assert.equal(nameKey("Ärne Vogt"), "Arne Vogt");
  assert.equal(nameKey("Jürgen Großmann"), "Jurgen Grosmann");
  assert.equal(nameKey("ÜBER äöü ẞ"), "UBER aou S");
  assert.equal(nameKey("Olaf Christ"), "Olaf Christ");
});

test("the kinship list: the living by name first, then the others, without the person", () => {
  assert.deepEqual(kinChoices(PEDIGREE, AGES, 9), [
    // Olaf < Omer (Ömer) < Otto by the key; Moller Kai twice and Möller Kai share a key:
    // the name itself puts Moller first, the smaller sim 7 before 8.
    { sim: 7, label: "Moller Kai, 31" },
    { sim: 8, label: "Moller Kai, 32" },
    { sim: 6, label: "Möller Kai, 30" },
    { sim: 3, label: "Olaf Christ, 40" },
    { sim: 2, label: "Ömer Bauer, 21" },
    { sim: 1, label: "Otto Ast, 105" },
    // The others: Anna < Arne (Ärne) < Bert, the dead with †, who moved away with the word.
    { sim: 4, label: "Anna Zeh, †" },
    { sim: 10, label: "Ärne Vogt, †" },
    { sim: 5, label: "Bert Yu, weggezogen" },
  ]);
});

test("the person asking is not in the list", () => {
  const sims = kinChoices(PEDIGREE, AGES, 1).map((choice) => choice.sim);
  assert.equal(sims.includes(1), false);
  assert.equal(sims.length, 9);
});
