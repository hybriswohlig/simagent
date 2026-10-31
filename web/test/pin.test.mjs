// A family pinned to the run page (ADR-0021 sections 2 and 6, Grok round 29): the key of
// its person, per run, in the browser's storage; no storage, or one that throws, pins
// nothing and fails nothing.
import { test } from "node:test";
import assert from "node:assert/strict";

import { pin, pinFork, pinnedKey, unpin } from "../dist/pin.js";

/** A storage of the browser, in memory. */
function memory() {
  const items = new Map();
  return {
    getItem: (key) => (items.has(key) ? items.get(key) : null),
    setItem: (key, value) => void items.set(key, String(value)),
    removeItem: (key) => void items.delete(key),
  };
}

test("a pinned person per run, until it is unpinned", () => {
  const storage = memory();
  assert.equal(pinnedKey(storage, "town"), null);
  pin(storage, "town", "00000000000000aa");
  pin(storage, "gabel", "00000000000000bb");
  assert.equal(pinnedKey(storage, "town"), "00000000000000aa");
  assert.equal(pinnedKey(storage, "gabel"), "00000000000000bb");
  pin(storage, "town", "00000000000000cc");
  assert.equal(pinnedKey(storage, "town"), "00000000000000cc");
  unpin(storage, "town");
  assert.equal(pinnedKey(storage, "town"), null);
  assert.equal(pinnedKey(storage, "gabel"), "00000000000000bb");
});

test("without a storage, or with one that throws, nothing is pinned and nothing fails", () => {
  const throwing = {
    getItem: () => {
      throw new Error("denied");
    },
    setItem: () => {
      throw new Error("denied");
    },
    removeItem: () => {
      throw new Error("denied");
    },
  };
  for (const storage of [null, throwing]) {
    assert.doesNotThrow(() => pin(storage, "town", "00000000000000aa"));
    assert.doesNotThrow(() => unpin(storage, "town"));
    assert.equal(pinnedKey(storage, "town"), null);
  }
});

test("a fork pins the family its run pins, and nothing where its run pins nothing", () => {
  const storage = memory();
  pin(storage, "town", "00000000000000aa");
  pinFork(storage, "town", "gabel");
  assert.equal(pinnedKey(storage, "gabel"), "00000000000000aa");
  assert.equal(pinnedKey(storage, "town"), "00000000000000aa");
  // A run of that name pinned before (and gone since) keeps nothing of it.
  pin(storage, "alt", "00000000000000bb");
  pinFork(storage, "leer", "alt");
  assert.equal(pinnedKey(storage, "alt"), null);
  assert.doesNotThrow(() => pinFork(null, "town", "gabel"));
});
