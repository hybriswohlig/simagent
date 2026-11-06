// The sentences of the wish for children (ADR-0017, addendum of 2026-11-05): the drawn wish,
// then for an open partnership the first sentence that applies; none while she is pregnant;
// written by hand from the texts of the design.
import { test } from "node:test";
import assert from "node:assert/strict";

import { wishLines } from "../dist/wish.js";

const names = { 1: "Anna Meier", 2: "Paul Bauer" };
const name = (sim) => names[sim];
const couple = (state, grown_at = state === "grown" ? 18 : null) => ({ woman: 1, man: 2, state, grown_at });

test("the drawn wish in three forms, none below the market age", () => {
  assert.deepEqual(wishLines("Anna Meier", { wish: 0, couple: null }, name), ["Anna Meier wünscht sich keine Kinder."]);
  assert.deepEqual(wishLines("Anna Meier", { wish: 1, couple: null }, name), ["Anna Meier wünscht sich ein Kind."]);
  assert.deepEqual(wishLines("Anna Meier", { wish: 3, couple: null }, name), ["Anna Meier wünscht sich 3 Kinder."]);
  assert.deepEqual(wishLines("Anna Meier", { wish: null, couple: null }, name), []);
});

test("the couple's sentence: hers and his, a grown child, the pause, the end, fulfilled", () => {
  const second = (state) => wishLines("Paul Bauer", { wish: 2, couple: couple(state) }, name)[1];
  assert.equal(second("hers"), "Anna Meier und Paul Bauer versuchen, ein Kind zu bekommen.");
  assert.equal(second("his"), "Anna Meier und Paul Bauer versuchen, ein Kind zu bekommen. Paul Bauer wünscht sich noch Kinder.");
  assert.equal(
    second("grown"),
    "Anna Meier und Paul Bauer versuchen, ein Kind zu bekommen. Ein Kind von Anna Meier ist 18, deshalb versuchen sie es noch einmal.",
  );
  // A town with its own age a child stops counting at names it (the server's number).
  assert.equal(
    wishLines("Paul Bauer", { wish: 2, couple: couple("grown", 16) }, name)[1],
    "Anna Meier und Paul Bauer versuchen, ein Kind zu bekommen. Ein Kind von Anna Meier ist 16, deshalb versuchen sie es noch einmal.",
  );
  assert.equal(second("pause"), "Nach der letzten Geburt versuchen sie es noch nicht wieder.");
  assert.equal(second("fertility_over"), "Anna Meier kann keine Kinder mehr bekommen.");
  assert.equal(second("fulfilled"), "Der Kinderwunsch von Anna Meier ist erfüllt.");
});

test("no sentence of the couple while she is pregnant", () => {
  assert.deepEqual(wishLines("Anna Meier", { wish: 2, couple: couple("pregnant") }, name), ["Anna Meier wünscht sich 2 Kinder."]);
});
