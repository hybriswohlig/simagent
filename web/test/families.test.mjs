// The families for moving one into a house (the playtest of 2026-11-04) on a hand-written
// pedigree and people answer: the households of the living by number, each member with its age
// from the people answer, the oldest first; at most as many members as a house holds; with
// children only, those where a member's mother, father or social parent is a member too.
import { test } from "node:test";
import assert from "node:assert/strict";

import { families } from "../dist/families.js";

function person(sim, household, parents = {}) {
  return {
    sim, lineage_key: `k${sim}`, sex: "female", alleles: [], born: 0, died: null, cause_of_death: null,
    emigrated: null, mother: null, father: null, social_parents: [], household, moved_out: false,
    carer: null, diseases: [], pool: null, first_name: `P${sim}`, surname: "S", ...parents,
  };
}

const entry = (sim, age) => ({ sim, pregnant: false, shown: { at: { t: 0, age_years: age, calendar_year: 0 }, body: {} }, looks: {}, traits: {} });

// Household 3: a couple (1, 2) with a daughter (4) and a foster son (5); household 7: two
// adults; household 9: three adults and a child of theirs, too many for a house of 3; sim 6 of
// household 7 has died (not in the people answer).
const PEDIGREE = {
  persons: [
    person(1, 3), person(2, 3), person(4, 3, { mother: 1, father: 2 }), person(5, 3, { social_parents: [1] }),
    person(6, 7), person(8, 7), person(10, 7, { mother: 6 }),
    person(11, 9), person(12, 9), person(13, 9), person(14, 9, { father: 11 }),
  ],
  partnerships: [],
  households: [],
};
const PEOPLE = [entry(1, 33), entry(2, 31), entry(4, 9), entry(5, 4), entry(8, 40), entry(10, 12), entry(11, 50), entry(12, 48), entry(13, 20), entry(14, 15)];

const seen = (kept) => kept.map((family) => [family.household, family.members.map((member) => [member.person.sim, member.age])]);

test("every family of the living, the oldest first", () => {
  assert.deepEqual(seen(families(PEDIGREE, PEOPLE, false, 8)), [
    [3, [[1, 33], [2, 31], [4, 9], [5, 4]]],
    [7, [[8, 40], [10, 12]]],
    [9, [[11, 50], [12, 48], [13, 20], [14, 15]]],
  ]);
});

test("only those with children, only those a house holds", () => {
  // Household 7's child has its mother 6 no more among the living members.
  assert.deepEqual(seen(families(PEDIGREE, PEOPLE, true, 8)).map(([household]) => household), [3, 9]);
  assert.deepEqual(seen(families(PEDIGREE, PEOPLE, true, 3)).map(([household]) => household), []);
  assert.deepEqual(seen(families(PEDIGREE, PEOPLE, false, 3)).map(([household]) => household), [7]);
});
