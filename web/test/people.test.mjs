// The filter of the person list on a hand-written pedigree and people answer: every filter
// keeps what it names, an empty choice keeps everything, the filters hold together, and a
// sim the pedigree lacks is left out. Every expected value is written by hand.
import { test } from "node:test";
import assert from "node:assert/strict";

import { found, NO_FILTER, partnered, toggled } from "../dist/people.js";

/** A person of the pedigree format with only the fields the filter reads set by hand. */
function person(sim, sex) {
  return {
    sim, lineage_key: `k${sim}`, sex, alleles: [], born: 0, died: null, cause_of_death: null,
    emigrated: null, mother: null, father: null, social_parents: [], household: 1, moved_out: false,
    carer: null, diseases: [], pool: null, first_name: `P${sim}`, surname: "S",
  };
}

/** An entry of the people answer: its age now and its looks; the body is not read. */
function entry(sim, age, looks, pregnant = false) {
  const plain = {
    hair: "light_brown", eyes: "blue", skin: "light", curls: "straight", freckles: "none",
    height: "middle", cognition: "middle",
  };
  return { sim, pregnant, shown: { at: { t: 0, age_years: age, calendar_year: 0 }, body: {} }, looks: { ...plain, ...looks } };
}

// 1 and 2 live together, 3 and 4 have separated, 5 never had a partner; 9 is not in the
// pedigree.
const PEDIGREE = {
  format_version: 5,
  persons: [person(1, "female"), person(2, "male"), person(3, "female"), person(4, "male"), person(5, "female")],
  partnerships: [
    { partnership: 1, woman: 1, man: 2, formed: 0, ended: null, ended_by: null, kinship: 0 },
    { partnership: 2, woman: 3, man: 4, formed: 0, ended: 10, ended_by: "separation", kinship: 0 },
  ],
  households: [],
};
const ENTRIES = [
  entry(1, 30, { hair: "blond", height: "high" }),
  entry(2, 34, { hair: "dark_brown", cognition: "high" }),
  entry(3, 22, { hair: "red", eyes: "green", freckles: "many" }),
  entry(4, 45, { hair: "blond", eyes: "brown" }),
  entry(5, 17, { skin: "beige", curls: "curly" }, true),
  entry(9, 20, {}),
];

const sims = (filter) => found(PEDIGREE, ENTRIES, filter).map((f) => f.person.sim);

test("without a filter everybody of the pedigree comes, in the order of the answer", () => {
  assert.deepEqual(sims(NO_FILTER), [1, 2, 3, 4, 5]);
});

test("the sex, the ages from and to, both bounds inside", () => {
  assert.deepEqual(sims({ ...NO_FILTER, sex: "female" }), [1, 3, 5]);
  assert.deepEqual(sims({ ...NO_FILTER, sex: "male" }), [2, 4]);
  assert.deepEqual(sims({ ...NO_FILTER, ageFrom: 22, ageTo: 34 }), [1, 2, 3]);
  assert.deepEqual(sims({ ...NO_FILTER, ageFrom: 40 }), [4]);
  assert.deepEqual(sims({ ...NO_FILTER, ageTo: 17 }), [5]);
});

test("single means no open partnership; an ended one does not count", () => {
  assert.deepEqual([...partnered(PEDIGREE)].sort(), [1, 2]);
  assert.deepEqual(sims({ ...NO_FILTER, singleOnly: true }), [3, 4, 5]);
});

test("pregnant only keeps the women the core names pregnant", () => {
  assert.deepEqual(sims({ ...NO_FILTER, pregnantOnly: true }), [5]);
  assert.deepEqual(sims({ ...NO_FILTER, pregnantOnly: true, ageFrom: 18 }), []);
});

test("a choice of words keeps those words, an empty choice keeps all", () => {
  const blondOrRed = toggled(toggled(new Set(), "blond"), "red");
  assert.deepEqual(sims({ ...NO_FILTER, hair: blondOrRed }), [1, 3, 4]);
  assert.deepEqual(sims({ ...NO_FILTER, hair: toggled(blondOrRed, "blond") }), [3]);
  assert.deepEqual(sims({ ...NO_FILTER, eyes: new Set(["green", "brown"]) }), [3, 4]);
  assert.deepEqual(sims({ ...NO_FILTER, skin: new Set(["beige"]) }), [5]);
  assert.deepEqual(sims({ ...NO_FILTER, curls: new Set(["curly"]) }), [5]);
  assert.deepEqual(sims({ ...NO_FILTER, freckles: new Set(["many"]) }), [3]);
  assert.deepEqual(sims({ ...NO_FILTER, height: new Set(["high"]) }), [1]);
  assert.deepEqual(sims({ ...NO_FILTER, cognition: new Set(["high", "rather_high"]) }), [2]);
});

test("the filters hold together", () => {
  const blond = new Set(["blond"]);
  assert.deepEqual(sims({ ...NO_FILTER, hair: blond, sex: "male" }), [4]);
  assert.deepEqual(sims({ ...NO_FILTER, hair: blond, singleOnly: true }), [4]);
  assert.deepEqual(sims({ ...NO_FILTER, hair: blond, ageTo: 29 }), []);
});
