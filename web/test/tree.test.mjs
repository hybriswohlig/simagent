// The tree around a person (P11) on a hand-written pedigree: the hourglass holds exactly the
// people its links reach, each once, and the layout puts every parent above its children.
import { test } from "node:test";
import assert from "node:assert/strict";

import { layout, MARK_DY, RADIUS } from "../dist/render/tree.js";
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

/** Between the same two rows no two lines cross. */
function assertNoCrossing(edges) {
  for (const a of edges) {
    for (const b of edges.filter((other) => other.y1 === a.y1 && other.y2 === a.y2)) {
      assert.ok((a.x1 - b.x1) * (a.x2 - b.x2) >= 0, `${a.from}-${a.to} crosses ${b.from}-${b.to}`);
    }
  }
}

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

// The fourth playtest (P1): rows in the pedigree's order put a grandchild of the second child
// before one of the first, and lines crossed. Each row stands in the order of its relatives
// in the row nearer the centre: children under their parents, ancestors above their
// children; the pedigree's order only among people of the same place.
test("children stand under their parents and ancestors above their children", () => {
  const family = {
    ...PEDIGREE,
    persons: [
      person(20, "female", null, null), // the father's mother
      person(21, "male", null, null), // the father's father
      person(22, "female", null, null), // the mother's mother
      person(23, "male", null, null), // the mother's father
      person(24, "female", 22, 23), // the mother
      person(25, "male", 20, 21), // the father
      person(26, "female", 24, 25), // the centre
      person(27, "male", null, null), // her partner
      person(28, "female", 26, 27), // the first child
      person(29, "male", 26, 27), // the second child
      person(30, "female", null, 29), // the second child's daughter, born first
      person(31, "male", 28, null), // the first child's son
      person(32, "female", null, 29), // the second child's second daughter
    ],
  };
  const glass = hourglass(family, 26);
  assert.deepEqual(sims(glass.up), [[24, 25], [22, 23, 20, 21], []]);
  assert.deepEqual(sims(glass.down), [[28, 29], [31, 30, 32]]);
  assertNoCrossing(layout(glass).edges);
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

test("a row follows its parents, not the order of the pedigree", () => {
  // Two grandchildren of 7 listed in the pedigree in the opposite order of their parents
  // (until the fourth playtest each row kept the pedigree's order, and their lines crossed).
  const late = { ...PEDIGREE, persons: [...PEDIGREE.persons, person(15, "male", 10, null)] };
  const reordered = { ...late, persons: [late.persons[14], ...late.persons.slice(0, 14)] };
  assert.deepEqual(sims(hourglass(reordered, 7).down), [[9, 10], [11, 15]]);
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
  // 20 and 30 have their first child at the same place; 20 has a second further right and
  // stands to the right of 30 (Grok's review), so that no lines cross.
  assert.deepEqual(sims(glass.up), [[21, 22], [24, 25], [30, 20, 31]]);
  assertNoCrossing(layout(glass).edges);
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
  // One row of 100, the mark under the name inside it (the third playtest, K7: room for the
  // lines between two rows below the names).
  assert.equal(alone.height, 100);
  assert.ok(parentsOnly.height > alone.height);
});

// The review of F6: the mark under the name of the last row ("†" or "weggezogen") lay below
// the edge of the drawing and was cut off; its baseline stays 6 px above the edge, room for
// the descenders of a 10 px font.
test("the mark under the lowest name fits in the drawing", () => {
  for (const sim of [7, 9, 14]) {
    const { nodes, height } = layout(hourglass(PEDIGREE, sim));
    const lowest = Math.max(...nodes.map((n) => n.y));
    assert.ok(lowest + MARK_DY + 6 <= height, `sim ${sim}`);
  }
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
  // Astra's review: the drawing links them too, not only the rows (and 23 to its child 25
  // by sim, below it).
  const edges = layout(glass).edges.map((edge) => [edge.from, edge.to]);
  assert.deepEqual(edges, [
    [21, 23],
    [22, 23],
    [23, 25],
  ]);
  assert.deepEqual(
    layout(hourglass(pedigree, 21)).edges.map((edge) => [edge.from, edge.to]),
    [
      [21, 23],
      [21, 24],
      [23, 25],
    ],
  );
});

// The playtest before the fun test (F6, Grok round 31): the dead and those who moved away are
// marked in the tree, shown short and spoken in full; a person's partner is the one of its
// partnership that has not ended.
test("the dead and those who moved away are marked", async () => {
  const { nodeMark } = await import("../dist/tree.js");
  const named = (fields) => ({ ...person(1, "female", null, null), first_name: "Pia", surname: "Heller", ...fields });
  assert.deepEqual(nodeMark(named({})), { shown: null, spoken: "Pia Heller" });
  assert.deepEqual(nodeMark(named({ died: 100 })), { shown: "†", spoken: "Pia Heller, gestorben" });
  assert.deepEqual(nodeMark(named({ emigrated: 100 })), { shown: "weggezogen", spoken: "Pia Heller, weggezogen" });
});

test("the partner is the one of the partnership that has not ended", async () => {
  const { currentPartner } = await import("../dist/tree.js");
  const couple = (partnership, woman, man, ended) => ({
    partnership, woman, man, formed: 0, ended, ended_by: ended === null ? null : "separation", kinship: 0,
  });
  const pedigree = {
    ...PEDIGREE,
    partnerships: [couple(1, 7, 6, 50), couple(2, 7, 8, null), couple(3, 10, 2, null)],
  };
  const of = (sim) => currentPartner(pedigree, pedigree.persons.find((p) => p.sim === sim))?.sim;
  assert.equal(of(7), 8);
  assert.equal(of(8), 7);
  assert.equal(of(6), undefined);
  assert.equal(of(14), undefined);
});

// The third playtest (K7): a line from a mother to her fifth child ran through the first name
// and the "†" of the child's aunt, drawn beside the mother. A line leaves its parent below the
// parent's name and mark and ends on top of the child's circle; on its way it meets no circle,
// name or mark of anyone else. (Only a link over two rows, of a person who is a parent and a
// grandparent at once, can still cross the row between.)
test("no line runs through a circle, a name or a mark of another person", () => {
  const dead = (p) => ({ ...p, died: 1 });
  const away = (p) => ({ ...p, emigrated: 1 });
  // 60 has the daughters 61 (moved away) and 62 (dead); 61 has five children.
  const family = {
    ...PEDIGREE,
    persons: [
      dead(person(60, "female", null, null)),
      away(person(61, "female", 60, null)),
      dead(person(62, "female", 60, null)),
      person(63, "female", 61, null),
      person(64, "male", 61, null),
      person(65, "female", 61, null),
      away(person(66, "male", 61, null)),
      person(67, "male", 61, null),
    ],
  };
  // Half the width of a first name or a mark: less than half the distance of two people.
  const HALF = 40;
  for (const sim of [60, 61, 63]) {
    const { nodes, edges } = layout(hourglass(family, sim));
    const at = new Map(nodes.map((n) => [n.person.sim, n]));
    assert.ok(edges.length > 0);
    for (const edge of edges) {
      const [from, to] = [at.get(edge.from), at.get(edge.to)];
      assert.equal(edge.x1, from.x);
      assert.ok(edge.y1 >= from.y + MARK_DY + 6, `${edge.from}>${edge.to} leaves below the mark`);
      assert.equal(edge.x2, to.x);
      assert.equal(edge.y2, to.y - RADIUS, `${edge.from}>${edge.to} ends on top of the circle`);
      for (const other of nodes.filter((n) => n !== from && n !== to)) {
        for (let i = 0; i <= 100; i += 1) {
          const x = edge.x1 + ((edge.x2 - edge.x1) * i) / 100;
          const y = edge.y1 + ((edge.y2 - edge.y1) * i) / 100;
          const inside = Math.abs(x - other.x) <= HALF && y >= other.y - RADIUS && y <= other.y + MARK_DY + 6;
          assert.ok(!inside, `${edge.from}>${edge.to} meets ${other.person.sim} at ${x},${y} (sim ${sim})`);
        }
      }
    }
  }
});
