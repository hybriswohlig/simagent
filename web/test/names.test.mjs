// A person's name is the export's first name and surname; a pedigree's names by sim.
import { test } from "node:test";
import assert from "node:assert/strict";

import { fullName, namesOf } from "../dist/names.js";

test("a name is the first name and the surname, by sim", () => {
  const person = (sim, first_name, surname) => ({ sim, first_name, surname });
  assert.equal(fullName(person(1, "Anna", "Wolf")), "Anna Wolf");
  const names = namesOf({ persons: [person(1, "Anna", "Wolf"), person(7, "Günter", "Stolz")] });
  assert.deepEqual([...names], [
    [1, "Anna Wolf"],
    [7, "Günter Stolz"],
  ]);
});
