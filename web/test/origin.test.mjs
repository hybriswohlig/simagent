// The sentences of the card's origin (Grok's rounds 43, R3, and 45, H2 to H5), each written
// by hand from Grok's wording; the lines are the core's.
import { test } from "node:test";
import assert from "node:assert/strict";

import { originSentence } from "../dist/origin.js";

test("a look: the label, the word, where it comes from", () => {
  assert.equal(originSentence({ kind: "hair", word: "blond", source: "mother" }), "Haar: blond, eher von der Mutter.");
  assert.equal(originSentence({ kind: "eyes", word: "blue", source: "both" }), "Augen: blau, von beiden.");
  assert.equal(originSentence({ kind: "height", word: "rather_high", source: "father" }), "Größe: eher groß, eher vom Vater.");
  assert.equal(
    originSentence({ kind: "hair", word: "blond", source: "both_faint" }),
    "Haar: blond, von beiden, bei den Eltern kaum zu sehen.",
  );
  assert.equal(originSentence({ kind: "height", word: "high", source: "away" }), "Größe: groß, Eltern nicht in der Stadt.");
  assert.equal(originSentence({ kind: "skin", word: "very_light", source: "both" }), "Haut: sehr hell, von beiden.");
  assert.equal(originSentence({ kind: "curls", word: "kinky", source: "father" }), "Locken: kraus, eher vom Vater.");
  assert.equal(originSentence({ kind: "freckles", word: "many", source: "mother" }), "Sommersprossen: viele, eher von der Mutter.");
});

test("two copies carry the word, their line tells whence", () => {
  assert.equal(originSentence({ kind: "hair", word: "red", source: "copies" }), "Haar: rot.");
  assert.equal(originSentence({ kind: "eyes", word: "blue", source: "copies" }), "Augen: blau.");
  assert.equal(
    originSentence({ kind: "red_copies", copies: ["mother", "father"] }),
    "Rotes Haar: eine Kopie von der Mutter, eine vom Vater.",
  );
  assert.equal(
    originSentence({ kind: "blue_copies", copies: ["mother", "father"] }),
    "Blaue Augen: eine Kopie von der Mutter, eine vom Vater.",
  );
  assert.equal(
    originSentence({ kind: "red_copies", copies: ["mother", "new"] }),
    "Rotes Haar: eine Kopie von der Mutter, eine neu entstanden.",
  );
  assert.equal(
    originSentence({ kind: "blue_copies", copies: ["new", "father"] }),
    "Blaue Augen: eine neu entstanden, eine Kopie vom Vater.",
  );
  assert.equal(originSentence({ kind: "red_copies", copies: ["new", "new"] }), "Rotes Haar: beide neu entstanden.");
  assert.equal(originSentence({ kind: "red_copies", copies: ["mother"] }), "Anlage für rotes Haar: die Kopie von der Mutter.");
  assert.equal(originSentence({ kind: "blue_copies", copies: ["father"] }), "Anlage für helle Augen: die Kopie vom Vater.");
  assert.equal(originSentence({ kind: "red_copies", copies: ["new"] }), "Anlage für rotes Haar: die Kopie neu entstanden.");
  assert.equal(originSentence({ kind: "blue_copies", copies: ["new"] }), "Anlage für helle Augen: die Kopie neu entstanden.");
});

test("freckles with red hair, the face, the steps", () => {
  assert.equal(originSentence({ kind: "freckles_red", word: "many", copies: 2 }), "Sommersprossen: viele, mit dem roten Haar.");
  assert.equal(
    originSentence({ kind: "freckles_red", word: "clear", copies: 1 }),
    "Sommersprossen: deutlich, mit der Anlage für rotes Haar.",
  );
  assert.equal(originSentence({ kind: "face", axis: 8, up: true, source: "father" }), "Gesicht: lange Nase, eher vom Vater.");
  assert.equal(originSentence({ kind: "face", axis: 10, up: false, source: "both" }), "Gesicht: dünne Lippen, von beiden.");
  assert.equal(originSentence({ kind: "face", axis: 6, up: true, source: "mother" }), "Gesicht: weit stehende Augen, eher von der Mutter.");
  assert.equal(originSentence({ kind: "smart", step: 2, source: "father" }), "Verstand: klug, eher vom Vater.");
  assert.equal(originSentence({ kind: "strong", step: 3, source: "both" }), "Kraft: bärenstark, von beiden.");
  assert.equal(originSentence({ kind: "beauty", step: 2, source: "both" }), "Schönheit: hübsch, von beiden.");
  assert.equal(
    originSentence({ kind: "beauty", step: -3, source: "both_faint" }),
    "Schönheit: hässlich, von beiden, bei den Eltern kaum zu sehen.",
  );
});

test("a line the viewer does not know says so", () => {
  assert.equal(originSentence({ kind: "elsewhere" }), null);
});

// No freckles shown and a bent toward fewer (Grok's round 47, S1 and S2), each sentence by
// hand from Grok's wording.
test("no freckles: where the bent toward none comes from", () => {
  assert.equal(
    originSentence({ kind: "freckles", word: "none", source: "father" }),
    "Sommersprossen: keine, die Anlage dafür eher vom Vater.",
  );
  assert.equal(
    originSentence({ kind: "freckles", word: "none", source: "mother" }),
    "Sommersprossen: keine, die Anlage dafür eher von der Mutter.",
  );
  assert.equal(originSentence({ kind: "freckles", word: "none", source: "both" }), "Sommersprossen: keine, die Anlage dafür von beiden.");
  assert.equal(
    originSentence({ kind: "freckles", word: "none", source: "both_faint" }),
    "Sommersprossen: keine, die Anlage dafür von beiden, bei den Eltern kaum zu sehen.",
  );
  assert.equal(
    originSentence({ kind: "freckles", word: "none", source: "away" }),
    "Sommersprossen: keine, die Anlage dafür aus den beiden Kopien, Eltern nicht in der Stadt.",
  );
});
