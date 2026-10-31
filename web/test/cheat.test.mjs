// The cheat mode (ADR-0023, addendum of 2026-10-31): a switch of the viewer, off at first and
// kept per browser; no storage, or one that throws, keeps it off and fails nothing. On, a
// person with a partner can be parted from them.
import { test } from "node:test";
import assert from "node:assert/strict";

import { cheatOn, separation, setCheat } from "../dist/cheat.js";

/** A storage of the browser, in memory. */
function memory() {
  const items = new Map();
  return {
    getItem: (key) => (items.has(key) ? items.get(key) : null),
    setItem: (key, value) => void items.set(key, String(value)),
    removeItem: (key) => void items.delete(key),
  };
}

const throwing = {
  getItem: () => {
    throw new Error("blocked");
  },
  setItem: () => {
    throw new Error("blocked");
  },
  removeItem: () => {
    throw new Error("blocked");
  },
};

test("off at first, kept once switched", () => {
  const storage = memory();
  assert.equal(cheatOn(storage), false);
  setCheat(storage, true);
  assert.equal(cheatOn(storage), true);
  setCheat(storage, false);
  assert.equal(cheatOn(storage), false);
});

test("without a storage, or with one that throws, it stays off", () => {
  assert.equal(cheatOn(null), false);
  setCheat(null, true);
  assert.equal(cheatOn(throwing), false);
  setCheat(throwing, true);
  assert.equal(cheatOn(throwing), false);
});

test("the input that parts a couple names the woman and the man by their sex", () => {
  const her = { sim: 8, sex: "female" };
  const him = { sim: 4, sex: "male" };
  const expected = { separate_partnership: { woman: 8, man: 4 } };
  assert.deepEqual(separation(her, him), expected);
  assert.deepEqual(separation(him, her), expected);
  assert.equal(separation(her, { sim: 9, sex: "female" }), null);
});
