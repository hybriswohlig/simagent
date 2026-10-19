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
