// Which body values the viewer shows (ADR-0005 section 6): the same breaking stage with the
// same or an older version.
import { test } from "node:test";
import assert from "node:assert/strict";

import { readable } from "../dist/contract.js";
import { BODY_CONTRACT_VERSION } from "../dist/generated/body.js";

test("the same stage, the same or older version, is readable", () => {
  assert.equal(readable(BODY_CONTRACT_VERSION), true);
  for (const [data, own] of [["0.1.0", "0.1.2"], ["0.1.2", "0.1.2"], ["1.0.0", "1.3.0"], ["1.2.9", "1.3.0"]]) {
    assert.equal(readable(data, own), true, `${data} by ${own}`);
  }
});

test("a newer version, another stage or no version is refused", () => {
  for (const [data, own] of [
    ["0.1.3", "0.1.2"],
    ["0.2.0", "0.1.9"],
    ["0.1.0", "0.2.0"],
    ["2.0.0", "1.9.9"],
    ["1.0.0", "0.9.0"],
    ["1.4.0", "1.3.0"],
    ["0.1", "0.1.0"],
    ["v0.1.0", "0.1.0"],
    ["0.01.0", "0.1.0"],
    ["", "0.1.0"],
  ]) {
    assert.equal(readable(data, own), false, `${data} by ${own}`);
  }
});
