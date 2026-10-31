// The line of a kinship (Grok's G6, K3 and K5): the blood word, the foster word beside it,
// and r in percent with one decimal in German digits, always, 0,0 included; without a blood
// relation the foster word alone, or "nicht verwandt". The lines are written by hand from
// those decisions.
import { test } from "node:test";
import assert from "node:assert/strict";

import { kinNodeText, kinshipText } from "../dist/kin.js";

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
  assert.equal(kinshipText(kin("unrelated", "foster_son", 0)), "Pflegesohn, 0,0 Prozent");
});

test("no relation", () => {
  assert.equal(kinshipText(kin("unrelated", null, 0)), "nicht verwandt, 0,0 Prozent");
});

test("a code this view does not know shows as the code", () => {
  assert.equal(kinshipText(kin("cousin_9", null, 0.8)), "kinship cousin_9, 0,8 Prozent");
});

// Under a name in the family tree (Grok's round 46, P3), written by hand from that decision:
// the word with its grade and "Halb-", without the generation (the row shows it), the foster
// word after it, then "·" and r with one decimal and "%".
test("the kinship under a name in the tree", () => {
  assert.equal(kinNodeText(kin("first_cousin_female", null, 12.5)), "Cousine 1. Grades · 12,5 %");
  assert.equal(kinNodeText(kin("first_cousin_male_older1", null, 6.3)), "Cousin 1. Grades · 6,3 %");
  assert.equal(kinNodeText(kin("half_sister", null, 25)), "Halbschwester · 25,0 %");
  assert.equal(kinNodeText(kin("daughter", "foster_daughter", 50)), "Tochter, Pflegetochter · 50,0 %");
  assert.equal(kinNodeText(kin("unrelated", "foster_mother", 0)), "Pflegemutter · 0,0 %");
  assert.equal(kinNodeText(kin("great_grandmother", null, 12.5)), "Urgroßmutter · 12,5 %");
});
