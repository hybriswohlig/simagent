// A person's name is the export's first name and surname; a pedigree's names by sim.
import { test } from "node:test";
import assert from "node:assert/strict";

import { fullName, namesByKey, namesOf } from "../dist/names.js";

test("a name is the first name and the surname, by sim", () => {
  const person = (sim, first_name, surname) => ({ sim, first_name, surname });
  assert.equal(fullName(person(1, "Anna", "Wolf")), "Anna Wolf");
  const names = namesOf({ persons: [person(1, "Anna", "Wolf"), person(7, "Günter", "Stolz")] });
  assert.deepEqual([...names], [
    [1, "Anna Wolf"],
    [7, "Günter Stolz"],
  ]);
});

// ADR-0021 section 3: a comparison names persons by lineage key, each run from its own
// pedigree, with the sim of its page.
test("a pedigree's names and sims by lineage key", () => {
  const person = (sim, lineage_key, first_name, surname) => ({ sim, lineage_key, first_name, surname });
  const names = namesByKey({
    persons: [person(1, "00000000000000aa", "Anna", "Wolf"), person(7, "00000000000000bb", "Günter", "Stolz")],
  });
  assert.deepEqual([...names], [
    ["00000000000000aa", { name: "Anna Wolf", sim: 1 }],
    ["00000000000000bb", { name: "Günter Stolz", sim: 7 }],
  ]);
});
