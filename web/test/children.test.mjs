// The possible children of a couple (Grok's round 44, K4 and K5): the heading with both
// names, a daughter or a son, and the words of hair, eyes, skin and height; written by hand.
import { test } from "node:test";
import assert from "node:assert/strict";

import { childName, childrenHeading, childWords } from "../dist/children.js";

const child = (sex, looks) => ({
  sex, body: {}, traits: { cognition: 0, strength: 0, beauty: 0 },
  looks: { hair: "blond", eyes: "blue", skin: "light", curls: "straight", freckles: "none", height: "middle", cognition: "middle", ...looks },
});

test("the heading names mother and father", () => {
  assert.equal(childrenHeading("Anna Meier", "Paul Bauer"), "So könnten Kinder von Anna Meier und Paul Bauer aussehen");
});

test("a daughter or a son with the words of four looks", () => {
  assert.equal(childName(child("female", {})), "Tochter");
  assert.equal(childName(child("male", {})), "Sohn");
  assert.equal(childWords(child("female", {})), "Haar blond · Augen blau · Haut hell · Größe mittel");
  assert.equal(
    childWords(child("male", { hair: "red", eyes: "hazel", skin: "very_light", height: "high" })),
    "Haar rot · Augen haselnuss · Haut sehr hell · Größe groß",
  );
});
