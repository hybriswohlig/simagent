// Pairing two persons: who may be offered (living, the other sex, in no open partnership;
// age and kinship are the core's to judge), and the input with woman and man by sex.
import { test } from "node:test";
import assert from "node:assert/strict";

import { arrangement, candidates, openPartners } from "../dist/pairing.js";

/** A person of a pedigree; the values follow the format, they are not computed. */
function person(sim, sex, ended = {}) {
  return {
    sim,
    lineage_key: "00000000000000aa",
    sex,
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
    ...ended,
  };
}

function partnership(partnership, woman, man, ended) {
  return { partnership, woman, man, formed: 0, ended, ended_by: ended === null ? null : "separation", kinship: 0 };
}

const PEDIGREE = {
  format_version: 4,
  persons: [
    person(1, "female"),
    person(2, "male"),
    person(3, "female"),
    person(4, "male"),
    person(5, "male", { died: 9 }),
    person(6, "male", { emigrated: 9 }),
    person(7, "male"),
    person(8, "female"),
  ],
  partnerships: [partnership(1, 3, 4, null), partnership(2, 1, 7, 5)],
  households: [],
};

test("the persons in an open partnership", () => {
  assert.deepEqual([...openPartners(PEDIGREE)].sort(), [3, 4]);
});

test("the candidates are the living of the other sex in no open partnership", () => {
  assert.deepEqual(
    candidates(PEDIGREE, 1).map((p) => p.sim),
    [2, 7],
  );
  assert.deepEqual(
    candidates(PEDIGREE, 2).map((p) => p.sim),
    [1, 8],
  );
  assert.deepEqual(candidates(PEDIGREE, 3), [], "in an open partnership");
  assert.deepEqual(candidates(PEDIGREE, 5), [], "dead");
  assert.deepEqual(candidates(PEDIGREE, 6), [], "emigrated");
  assert.deepEqual(candidates(PEDIGREE, 99), [], "unknown");
});

test("the input names the woman and the man by their sex", () => {
  const woman = PEDIGREE.persons[0];
  const man = PEDIGREE.persons[1];
  const input = { arrange_partnership: { woman: 1, man: 2 } };
  assert.deepEqual(arrangement(woman, man), input);
  assert.deepEqual(arrangement(man, woman), input);
  assert.equal(arrangement(woman, PEDIGREE.persons[2]), null, "two women");
});
