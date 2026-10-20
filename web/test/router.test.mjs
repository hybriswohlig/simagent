// The routes of the viewer: each hash to its route and back, everything else not found.
import { test } from "node:test";
import assert from "node:assert/strict";

import { format, parse, runName } from "../dist/router.js";

test("each known hash is its route, and back", () => {
  const known = [
    ["#/", { page: "runs" }],
    ["#/portraits", { page: "portraits" }],
    ["#/runs/town-1", { page: "run", run: "town-1" }],
    ["#/runs/town_1.fork/persons/17", { page: "person", run: "town_1.fork", sim: "17" }],
    ["#/runs/a/persons/1/tree", { page: "tree", run: "a", sim: "1" }],
  ];
  for (const [hash, route] of known) {
    assert.deepEqual(parse(hash), route, hash);
    assert.equal(format(route), hash);
  }
  assert.deepEqual(parse(""), { page: "runs" });
  assert.deepEqual(parse("#"), { page: "runs" });
  assert.deepEqual(parse("/runs/a"), { page: "run", run: "a" });
});

test("anything else is not found and keeps its hash", () => {
  const unknown = [
    "#/runs",
    "#/runs/",
    "#/runs/.hidden",
    "#/runs/../etc",
    "#/runs/a%2Fb",
    "#/runs/a/persons",
    "#/runs/a/persons/0",
    "#/runs/a/persons/017",
    "#/runs/a/persons/x",
    "#/runs/a/persons/1/tree/x",
    "#/runs/a/persons/1/kin",
    "#/runs/a/people/1",
    "#runs/a",
    "#//runs/a",
    "#/portraits/",
    "#portraits",
  ];
  for (const hash of unknown) {
    assert.deepEqual(parse(hash), { page: "not_found", hash }, hash);
    assert.equal(format(parse(hash)), hash);
  }
});

test("a new run's name follows the rule of the run directory", () => {
  for (const name of ["stadt", "Stadt-2", "a.b_c", "0"]) {
    assert.equal(runName(name), true, name);
  }
  for (const name of ["", ".x", "a/b", "a b", "ä", "a%20"]) {
    assert.equal(runName(name), false, name);
  }
});
