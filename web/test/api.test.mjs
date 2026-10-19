// The viewer takes an answer only of the kind and version it expects (ADR-0021 section 2).
import { test } from "node:test";
import assert from "node:assert/strict";

import { UnknownAnswer, accept, getJson } from "../dist/api.js";

const SERVER = { format: "simagent.view.runs", version: 1 };
const FILE = { format_version: 4 };

test("an answer of the expected kind and version is taken", () => {
  const body = { format: "simagent.view.runs", version: 1, runs: [] };
  assert.equal(accept(body, SERVER), body);
  const file = { format_version: 4, persons: [] };
  assert.equal(accept(file, FILE), file);
});

test("another kind, version or shape is refused", () => {
  const refused = [
    [{ format: "simagent.view.runs", version: 2 }, SERVER],
    [{ format: "simagent.view.run", version: 1 }, SERVER],
    [{ format: "simagent.view.runs", version: "1" }, SERVER],
    [{ format: "simagent.view.runs" }, SERVER],
    [{ version: 1 }, SERVER],
    [{ format_version: 1 }, SERVER],
    [{ format_version: 5 }, FILE],
    [{ format_version: "4" }, FILE],
    [{ format: "simagent.view.runs", version: 1 }, FILE],
    [null, SERVER],
    [[], FILE],
    ["{}", FILE],
    [4, FILE],
  ];
  for (const [body, expected] of refused) {
    assert.throws(() => accept(body, expected), UnknownAnswer, JSON.stringify(body));
  }
  assert.throws(() => accept({ format_version: 5 }, FILE), {
    message: 'expected {"format_version":4}, came {"format_version":5}',
  });
});

test("only routes of the server are fetched", async () => {
  for (const path of ["https://example.org/api/runs", "//example.org/api/runs", "/runs"]) {
    await assert.rejects(getJson(path, SERVER), { message: `not a route of the server: ${path}` });
  }
});
