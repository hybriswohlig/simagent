// The page of the model's assumptions (Grok O1): its six sections in Grok's order, the formula
// r = 2φ in the section on kinship, and the exception of N1 in the last.
import { test } from "node:test";
import assert from "node:assert/strict";

import { ASSUMPTIONS } from "../dist/assumptions.js";

test("the six sections in order", () => {
  assert.deepEqual(
    ASSUMPTIONS.map((section) => section.heading),
    ["Die Modellbevölkerung", "Die vier Posten", "Größe", "Verwandtschaft", "Erbkrankheiten", "Herkunft einer übersprungenen Kopie"],
  );
  assert.ok(ASSUMPTIONS[0].paragraphs.includes("Keine dieser Zahlen ist eine IQ-Skala. Das gilt auch für die Kognition."));
  assert.ok(ASSUMPTIONS[3].paragraphs.some((paragraph) => paragraph.includes("r = 2φ")));
  assert.equal(ASSUMPTIONS[5].paragraphs.length, 1);
});
