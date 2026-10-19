// What the person page shows while it waits: only an answer for the run it shows, and a
// failure as a text under a key.
import { test } from "node:test";
import assert from "node:assert/strict";

import { UnknownAnswer } from "../dist/api.js";
import { current, failure } from "../dist/loaded.js";
import { TEXT } from "../dist/text.js";

test("an answer for another run is not shown", () => {
  const ready = { state: "ready", key: "a", value: 1 };
  assert.equal(current(ready, "a"), ready);
  assert.deepEqual(current(ready, "b"), { state: "loading", key: "b" });
  assert.deepEqual(current({ state: "failed", key: "a", text: "failed_network" }, "b"), { state: "loading", key: "b" });
});

test("a failure is a German text under a key", () => {
  assert.equal(failure(new UnknownAnswer("expected 4, came 5")), "failed_version");
  assert.equal(failure(new Error("Failed to fetch")), "failed_network");
  assert.equal(failure("odd"), "failed_network");
  for (const key of ["failed_version", "failed_network"]) {
    assert.ok(TEXT[key].length > 0 && !TEXT[key].includes("expected"), key);
  }
});
