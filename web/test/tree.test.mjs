// The tree around a person (P11) on a hand-written pedigree: the hourglass holds exactly the
// people its links reach, each once, and the layout puts every parent above its children.
import { test } from "node:test";
import assert from "node:assert/strict";

import { layout } from "../dist/render/tree.js";
import { hourglass } from "../dist/tree.js";

/** A person of the pedigree format with only the fields the tree reads set by hand. */
function person(sim, sex, mother, father) {
  return {
    sim, lineage_key: String(sim), sex, alleles: [], born: 0, died: null, cause_of_death: null,
    emigrated: null, mother, father, social_parents: [], household: 1, moved_out: false,
    carer: null, diseases: [], pool: null,
  };
}

// 1 and 2 are the parents of 5, 3 and 4 of 6; 5 and 6 of 7; 7 and 8 of 9 and 10; 9 the
// father of 11; 12 a half-sibling of 7 by 5 and 13; 14 a stranger.
const PEDIGREE = {
  format_version: 4,
  persons: [
    person(1, "female", null, null),
    person(2, "male", null, null),
    person(3, "female", null, null),
    person(4, "male", null, null),
    person(5, "female", 1, 2),
    person(6, "male", 3, 4),
    person(7, "female", 5, 6),
    person(8, "male", null, null),
    person(9, "male", 7, 8),
    person(10, "female", 7, 8),
    person(11, "female", null, 9),
    person(12, "male", 5, 13),
    person(13, "male", null, null),
    person(14, "female", null, null),
  ],
  partnerships: [],
  households: [],
};

const sims = (levels) => levels.map((level) => level.map((p) => p.sim));

test("the hourglass holds exactly the people its links reach", () => {
  const glass = hourglass(PEDIGREE, 7);
  assert.equal(glass.center.sim, 7);
  assert.deepEqual(sims(glass.up), [[5, 6], [1, 2, 3, 4], []]);
  assert.deepEqual(sims(glass.down), [[9, 10], [11]]);
  const top = hourglass(PEDIGREE, 1);
  assert.deepEqual(sims(top.up), [[], [], []]);
  assert.deepEqual(sims(top.down), [[5], [7, 12]]);
  assert.deepEqual(sims(hourglass(PEDIGREE, 14).down), [[], []]);
  assert.equal(hourglass(PEDIGREE, 99), null);
});

test("dangling parents are skipped and social parents are not biological", () => {
  const odd = {
    ...PEDIGREE,
    persons: [
      person(40, "male", null, null),
      { ...person(41, "female", 99, 40), social_parents: [42] },
      person(42, "female", null, null),
    ],
  };
  assert.deepEqual(sims(hourglass(odd, 41).up), [[40], [], []]);
  assert.deepEqual(sims(hourglass(odd, 42).down), [[], []]);
});

test("every row is in the order of the pedigree", () => {
  // Two children of 7 listed in the pedigree in the opposite order of their parents.
  const late = { ...PEDIGREE, persons: [...PEDIGREE.persons, person(15, "male", 10, null)] };
  const reordered = { ...late, persons: [late.persons[14], ...late.persons.slice(0, 14)] };
  assert.deepEqual(sims(hourglass(reordered, 7).down), [[9, 10], [15, 11]]);
});

test("a person reached twice is drawn once", () => {
  // 21 and 22 are cousins through 20; their child 23 has 20 twice among its forebears.
  const cousins = {
    ...PEDIGREE,
    persons: [
      person(20, "female", null, null),
      person(30, "male", null, null),
      person(31, "male", null, null),
      person(24, "female", 20, 30),
      person(25, "male", 20, 31),
      person(21, "female", 24, null),
      person(22, "male", 25, null),
      person(23, "female", 21, 22),
    ],
  };
  const glass = hourglass(cousins, 23);
  assert.deepEqual(sims(glass.up), [[21, 22], [24, 25], [20, 30, 31]]);
});

test("a shared ancestor at two depths sits at the deeper, above all its children", () => {
  // C (50) has parents A (51) and B (52); A's parent is X (53); B's parent is Y (54), whose
  // parent is X too.
  const shared = {
    ...PEDIGREE,
    persons: [
      person(53, "female", null, null),
      person(54, "male", 53, null),
      person(51, "female", 53, null),
      person(52, "male", 54, null),
      person(50, "female", 51, 52),
    ],
  };
  const glass = hourglass(shared, 50);
  assert.deepEqual(sims(glass.up), [[51, 52], [54], [53]]);
  const { nodes, edges } = layout(glass);
  const at = new Map(nodes.map((n) => [n.person.sim, n]));
  assert.deepEqual(edges.map((e) => `${e.from}>${e.to}`).sort(), ["51>50", "52>50", "53>51", "53>54", "54>52"]);
  for (const { from, to } of edges) {
    assert.ok(at.get(from).y < at.get(to).y, `${from} above ${to}`);
  }
});

test("the layout puts each parent above its children and draws each link once", () => {
  const glass = hourglass(PEDIGREE, 7);
  const { nodes, edges, width, height } = layout(glass);
  assert.deepEqual(nodes.map((n) => n.person.sim).sort((a, b) => a - b), [1, 2, 3, 4, 5, 6, 7, 9, 10, 11]);
  assert.deepEqual(nodes.filter((n) => n.center).map((n) => n.person.sim), [7]);
  const at = new Map(nodes.map((n) => [n.person.sim, n]));
  const places = new Set(nodes.map((n) => `${n.x},${n.y}`));
  assert.equal(places.size, nodes.length);
  const links = edges.map((e) => `${e.from}>${e.to}`).sort();
  assert.deepEqual(links, ["1>5", "2>5", "3>6", "4>6", "5>7", "6>7", "7>10", "7>9", "9>11"]);
  for (const { from, to } of edges) {
    assert.ok(at.get(from).y < at.get(to).y, `${from} above ${to}`);
  }
  for (const node of nodes) {
    assert.ok(node.x > 0 && node.x < width && node.y > 0 && node.y < height);
  }
});

test("empty generations above and below take no room", () => {
  const parentsOnly = layout(hourglass(PEDIGREE, 9));
  const top = layout(hourglass(PEDIGREE, 1));
  // 1 has no forebears: the rows begin with 1 itself.
  assert.equal(Math.min(...top.nodes.map((n) => n.y)), top.nodes.find((n) => n.center).y);
  // 14 is alone: one row.
  const alone = layout(hourglass(PEDIGREE, 14));
  assert.equal(alone.nodes.length, 1);
  assert.equal(alone.height, 80);
  assert.ok(parentsOnly.height > alone.height);
});

// The playtest before the fun test (F1): an arrival whose parents came from the pool too has
// no town parents by sim, only the keys of its pool origin; where a person of the pedigree
// has such a key, that person is its parent, in the hourglass and as mother or father.
test("parents from the pool by their keys", async () => {
  const { parentsOf } = await import("../dist/tree.js");
  const arrival = (sim, sex, mother_key, father_key) => ({
    ...person(sim, sex, null, null),
    pool: {
      genome_key: String(sim),
      born_pool: 1,
      arrived: 0,
      arrival_year: 0,
      mother_key,
      father_key,
      social_parent_keys: [],
    },
  });
  const pedigree = {
    format_version: 4,
    persons: [
      arrival(21, "female", null, null),
      arrival(22, "male", null, null),
      arrival(23, "female", "21", "22"),
      arrival(24, "male", "21", "99"),
      person(25, "female", 23, null),
    ],
    partnerships: [],
    households: [],
  };
  const [mother, father] = parentsOf(pedigree, pedigree.persons[2]);
  assert.deepEqual([mother?.sim, father?.sim], [21, 22]);
  // A key that names nobody of the pedigree is no parent here.
  const [his, none] = parentsOf(pedigree, pedigree.persons[3]);
  assert.deepEqual([his?.sim, none], [21, undefined]);
  const glass = hourglass(pedigree, 23);
  assert.deepEqual(
    glass.up.map((row) => row.map((p) => p.sim)),
    [[21, 22], [], []],
  );
  assert.deepEqual(
    hourglass(pedigree, 21).down.map((row) => row.map((p) => p.sim)),
    [[23, 24], [25]],
  );
});
