// The line of a kinship (Grok's G6 and K3): the blood word, the foster word beside it, and r
// in percent with one decimal in German digits; without a blood relation the foster word
// alone, or "nicht verwandt". The lines are written by hand from those decisions.
import { test } from "node:test";
import assert from "node:assert/strict";

import { kinshipText } from "../dist/kin.js";

const kin = (word, foster, r_percent) => ({ to: 2, word, foster, r_percent });

test("a blood relation with r in percent", () => {
  assert.equal(kinshipText(kin("uncle", null, 25)), "Onkel, 25,0 Prozent");
  assert.equal(kinshipText(kin("first_cousin_female", null, 12.5)), "Cousine 1. Grades, 12,5 Prozent");
  assert.equal(
    kinshipText(kin("first_cousin_male_older1", null, 6.3)),
    "Cousin 1. Grades, eine Generation älter, 6,3 Prozent",
  );
  assert.equal(kinshipText(kin("half_sister", null, 25)), "Halbschwester, 25,0 Prozent");
  assert.equal(kinshipText(kin("self", null, 106.3)), "selbst, 106,3 Prozent");
});

test("a foster word beside the blood word, or alone", () => {
  assert.equal(kinshipText(kin("grandmother", "foster_mother", 25)), "Großmutter, Pflegemutter, 25,0 Prozent");
  assert.equal(kinshipText(kin("unrelated", "foster_son", 0)), "Pflegesohn");
});

test("no relation", () => {
  assert.equal(kinshipText(kin("unrelated", null, 0)), "nicht verwandt");
});

test("a code this view does not know shows as the code", () => {
  assert.equal(kinshipText(kin("cousin_9", null, 0.8)), "kinship cousin_9, 0,8 Prozent");
});
