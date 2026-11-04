// The mood beside the points of "Warum?" (ADR-0026, addendum of 2026-11-04 on the mood): the
// server's number in percent with its sign, the typographic minus, zero without one, nothing
// for a command or a closed offer. Written by hand from the words of `text.ts`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { moodWords } from "../dist/why.js";

test("the mood with its sign, nothing without one", () => {
  assert.equal(moodWords(6), " (Laune +6 %)");
  assert.equal(moodWords(10), " (Laune +10 %)");
  assert.equal(moodWords(-3), " (Laune −3 %)");
  assert.equal(moodWords(-10), " (Laune −10 %)");
  assert.equal(moodWords(0), " (Laune 0 %)");
  assert.equal(moodWords(null), "");
});
