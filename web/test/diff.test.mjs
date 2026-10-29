// The comparison page's rows (ADR-0021 section 3 and the addenda G1 to G3 and of 2026-10-28;
// texts Grok rounds 28, 34 and 41): the diseases counted per run, the persons of one run only
// and, for each life that went otherwise, a row per field that differs with the text of each
// run, those with a hereditary disease first. The answer is written by hand after the golden
// comparison of the switch scenario (tests/golden/diff_switch.json), shortened; every
// expected text by hand from Grok's wording.
import { test } from "node:test";
import assert from "node:assert/strict";

import { diffView } from "../dist/diff.js";

const date = (year, month, day_of_month) => ({ year, day_of_year: 0, month, day_of_month, week: 0, weekday: 0, season: 0, second_of_day: 0 });
const at = (t, year, month, day) => ({ t, date: date(year, month, day) });
const life = (fields) => ({ born: at(0, 0, 0, 0), died: null, cause_of_death: null, emigrated: null, partnerships: [], children: 0, diseases: [], ...fields });
const named = (entries) => new Map(entries.map(([key, name, sim]) => [key, { name, sim }]));

const SON = "5e51a4f2e3aba5e9";
const HIS = "830458f975a6df91";
const HER = "935f6cce0d166101";
const ANSWER = {
  format: "simagent.view.diff",
  version: 3,
  a: "town",
  b: "gabel",
  summary: [],
  a_only: ["a4a0b110a6920c3e"],
  b_only: ["f62d292b56475e6e"],
  changed: [
    {
      lineage_key: SON,
      differs: ["died", "cause_of_death", "partnerships", "children"],
      hereditary: true,
      a: life({ born: at(29030400, 0, 11, 28), partnerships: [{ partner: HIS, formed: at(897523200, 28, 7, 0), ended: null, ended_by: null }], children: 2 }),
      b: life({ born: at(29030400, 0, 11, 28), died: at(485900217, 15, 5, 23), cause_of_death: "core:disease_ar" }),
    },
    {
      lineage_key: HER,
      differs: ["died", "cause_of_death", "partnerships"],
      hereditary: false,
      a: life({ partnerships: [{ partner: SON, formed: at(174182400, 5, 7, 0), ended: at(855601366, 27, 2, 3), ended_by: "separation" }] }),
      b: life({ died: at(562966012, 17, 11, 9), cause_of_death: "core:senescence", partnerships: [{ partner: SON, formed: at(174182400, 5, 7, 0), ended: at(562966012, 17, 11, 9), ended_by: "death" }] }),
    },
  ],
};
const NAMES = {
  a: named([[SON, "Karl Wolf", 7], [HIS, "Lena Berg", 9], [HER, "Ida Wolf", 3], ["a4a0b110a6920c3e", "Paul Wolf", 12]]),
  b: named([[SON, "Karl Wolf", 7], [HER, "Ida Wolf", 3], ["f62d292b56475e6e", "Mia Berg", 12]]),
};

test("the persons of one run only, by their names in that run", () => {
  const view = diffView(ANSWER, NAMES);
  assert.deepEqual(view.aOnly, [{ key: "a4a0b110a6920c3e", name: "Paul Wolf", sim: 12 }]);
  assert.deepEqual(view.bOnly, [{ key: "f62d292b56475e6e", name: "Mia Berg", sim: 12 }]);
});

test("a life that went otherwise is a row per field that differs", () => {
  const [son, her] = diffView(ANSWER, NAMES).changed;
  assert.deepEqual(son, {
    key: SON,
    name: "Karl Wolf",
    a: 7,
    b: 7,
    rows: [
      { label: "Tod", a: ["keiner"], b: ["Jahr 15, 6. Monat, 24. Tag"] },
      { label: "Todesursache", a: ["keine"], b: ["Zäher Atem"] },
      { label: "Partnerschaften", a: ["mit Lena Berg, ab Jahr 28, 8. Monat, 1. Tag"], b: ["keine"] },
      { label: "Kinder", a: ["2"], b: ["0"] },
    ],
  });
  assert.deepEqual(her.rows, [
    { label: "Tod", a: ["keiner"], b: ["Jahr 17, 12. Monat, 10. Tag"] },
    { label: "Todesursache", a: ["keine"], b: ["Alter"] },
    {
      label: "Partnerschaften",
      a: ["mit Karl Wolf, ab Jahr 5, 8. Monat, 1. Tag bis Jahr 27, 3. Monat, 4. Tag, durch Trennung"],
      b: ["mit Karl Wolf, ab Jahr 5, 8. Monat, 1. Tag bis Jahr 17, 12. Monat, 10. Tag, durch Tod"],
    },
  ]);
});

test("a person no pedigree names is the person of its key", () => {
  const view = diffView({ ...ANSWER, a_only: ["00000000000000ff"] }, { a: new Map(), b: new Map() });
  assert.deepEqual(view.aOnly, [{ key: "00000000000000ff", name: "Person 00000000000000ff", sim: null }]);
  assert.equal(view.changed[0].name, "Person 5e51a4f2e3aba5e9");
  assert.deepEqual(view.changed[0].rows[2].a, ["mit Person 830458f975a6df91, ab Jahr 28, 8. Monat, 1. Tag"]);
});

test("the other fields and an emigration", () => {
  const answer = {
    ...ANSWER,
    a_only: [],
    b_only: [],
    changed: [
      {
        lineage_key: SON,
        differs: ["born", "emigrated", "partnerships"],
        a: life({ born: at(1, 2, 0, 0), emigrated: at(5, 9, 1, 1), partnerships: [{ partner: HIS, formed: at(3, 4, 0, 0), ended: at(5, 9, 1, 1), ended_by: "emigration" }] }),
        b: life({ born: at(2, 3, 0, 0) }),
      },
    ],
  };
  assert.deepEqual(diffView(answer, NAMES).changed[0].rows, [
    { label: "Geburt", a: ["Jahr 2, 1. Monat, 1. Tag"], b: ["Jahr 3, 1. Monat, 1. Tag"] },
    { label: "Wegzug", a: ["Jahr 9, 2. Monat, 2. Tag"], b: ["keiner"] },
    { label: "Partnerschaften", a: ["mit Lena Berg, ab Jahr 4, 1. Monat, 1. Tag bis Jahr 9, 2. Monat, 2. Tag, durch Wegzug"], b: ["keine"] },
  ]);
});

// Astra's review of G3a: each column names and links from its own run; a person named only
// in `b` is named from there.
test("each column names and links from its own run", () => {
  const ROSA = "00000000000000cc";
  const answer = {
    ...ANSWER,
    a_only: [],
    b_only: [],
    changed: [
      {
        lineage_key: SON,
        differs: ["partnerships"],
        a: life({ partnerships: [{ partner: HIS, formed: at(1, 2, 0, 0), ended: null, ended_by: null }] }),
        b: life({ partnerships: [{ partner: ROSA, formed: at(1, 2, 0, 0), ended: null, ended_by: null }] }),
      },
    ],
  };
  const both = {
    a: named([[SON, "Karl Wolf", 7], [HIS, "Lena Berg", 9]]),
    b: named([[SON, "Karl Wolf", 21], [ROSA, "Rosa Lang", 22]]),
  };
  assert.deepEqual(diffView(answer, both).changed, [
    {
      key: SON,
      name: "Karl Wolf",
      a: 7,
      b: 21,
      rows: [
        {
          label: "Partnerschaften",
          a: ["mit Lena Berg, ab Jahr 2, 1. Monat, 1. Tag"],
          b: ["mit Rosa Lang, ab Jahr 2, 1. Monat, 1. Tag"],
        },
      ],
    },
  ]);
  const [only] = diffView(answer, { a: new Map(), b: both.b }).changed;
  assert.deepEqual([only.name, only.a, only.b], ["Karl Wolf", null, 21]);
});

// The playtest before the fun test (F3): the comparison page says where each run stands
// (Grok round 30, W3).
test("the years of the runs", async () => {
  const { years } = await import("../dist/diff.js");
  assert.deepEqual(years("town", 50, "gabel", 79), [
    { run: "town" },
    " steht im Jahr 50, ",
    { run: "gabel" },
    " im Jahr 79.",
  ]);
});

// Grok's round 42 (D2): runs at different seconds are compared at the earlier one, and the
// page says so, with both runs as links and the date the server gives; nothing when nothing
// was recomputed.
test("the second of the comparison, when one run was recomputed", async () => {
  const { comparedAt } = await import("../dist/diff.js");
  const at = { t: 2_547_417_599, date: { year: 80, day_of_year: 363, month: 12, day_of_month: 27, week: 51, weekday: 6, season: 3, second_of_day: 86_399 } };
  const sentence = [
    "Verglichen wird zum Stand von ",
    { run: "aus" },
    " (Jahr 80, 13. Monat, 28. Tag); ",
    { run: "an" },
    " ist dafür bis dorthin nachgerechnet, an ihm selbst ändert sich nichts.",
  ];
  assert.deepEqual(comparedAt({ a: "aus", b: "an", at, recomputed: "an" }), sentence);
  assert.deepEqual(comparedAt({ a: "an", b: "aus", at, recomputed: "an" }), sentence);
  assert.equal(comparedAt({ a: "aus", b: "an", at, recomputed: null }), null);
});

// Grok's round 34 (E2, E3): the row "Erbkrankheiten", each disease that shows on its own line
// with the year and the age of its onset as card and chronicle say them ("mit 1 Jahr"), at 0
// years "seit der Geburt"; without one "keine".
test("the hereditary diseases of a life", () => {
  const answer = {
    ...ANSWER,
    a_only: [],
    b_only: [],
    changed: [
      {
        lineage_key: SON,
        differs: ["diseases"],
        a: life({
          diseases: [
            { disease: "core:disease_ar", year: 34, age_years: 34, rests: false },
            { disease: "core:disease_ad_late", year: 12, age_years: 1, rests: false },
          ],
        }),
        b: life({ diseases: [{ disease: "core:disease_ar", year: -1, age_years: 0, rests: false }] }),
        hereditary: true,
      },
      {
        lineage_key: HIS,
        differs: ["diseases"],
        hereditary: true,
        a: life({}),
        b: life({ diseases: [{ disease: "core:disease_xr", year: 40, age_years: 2, rests: false }] }),
      },
    ],
  };
  const view = diffView(answer, { a: new Map(), b: new Map() });
  assert.deepEqual(
    view.changed.map((entry) => entry.rows),
    [
      [
        {
          label: "Erbkrankheiten",
          a: ["Zäher Atem, seit Jahr 34, mit 34 Jahren", "Später Verfall, seit Jahr 12, mit 1 Jahr"],
          b: ["Zäher Atem, seit der Geburt"],
        },
      ],
      [{ label: "Erbkrankheiten", a: ["keine"], b: ["Schwere Blutung, seit Jahr 40, mit 2 Jahren"] }],
    ],
  );
});

// Grok's round 41, V2: a disease whose onset is reached while the switch is off rests.
test("a disease at rest is named as resting", () => {
  const answer = {
    ...ANSWER,
    a_only: [],
    b_only: [],
    changed: [
      {
        lineage_key: HER,
        differs: ["diseases"],
        hereditary: true,
        a: life({ diseases: [{ disease: "core:disease_liability", year: 33, age_years: 32, rests: false }] }),
        b: life({ diseases: [{ disease: "core:disease_liability", year: 33, age_years: 32, rests: true }] }),
      },
    ],
  };
  const [her] = diffView(answer, { a: new Map(), b: new Map() }).changed;
  assert.deepEqual(her.rows, [
    { label: "Erbkrankheiten", a: ["Geteilte Schwere, seit Jahr 33, mit 32 Jahren"], b: ["Geteilte Schwere, Anlage, ruht"] },
  ]);
});

// Grok's round 41, V1: the counts per disease and run, the name in the nominative, a code
// without a name as the code.
test("the hereditary diseases counted per run", () => {
  const answer = {
    ...ANSWER,
    summary: [
      { disease: "core:disease_ar", a: { affected: 1, died: 0 }, b: { affected: 0, died: 2 } },
      { disease: "core:disease_new", a: { affected: 0, died: 0 }, b: { affected: 3, died: 1 } },
    ],
  };
  assert.deepEqual(diffView(answer, NAMES).summary, [
    { name: "Zäher Atem", a: "1 betroffen, 0 daran gestorben", b: "0 betroffen, 2 daran gestorben" },
    { name: "core:disease_new", a: "0 betroffen, 0 daran gestorben", b: "3 betroffen, 1 daran gestorben" },
  ]);
  assert.deepEqual(diffView(ANSWER, NAMES).summary, []);
});

// Grok's round 41, V1: the lives with a hereditary disease first, each group in the order of
// the answer; an empty group goes; the others alone keep the heading "Anders verlaufen".
test("the lives with a hereditary disease first", () => {
  const titles = (answer) => diffView(answer, NAMES).groups.map((group) => [group.title, group.lives.map((each) => each.key)]);
  assert.deepEqual(titles(ANSWER), [
    ["Mit einem Unterschied der Erbkrankheiten", [SON]],
    ["Im Übrigen anders verlaufen", [HER]],
  ]);
  const [son, her] = ANSWER.changed;
  assert.deepEqual(titles({ ...ANSWER, changed: [son] }), [["Mit einem Unterschied der Erbkrankheiten", [SON]]]);
  assert.deepEqual(titles({ ...ANSWER, changed: [her] }), [["Anders verlaufen", [HER]]]);
  assert.deepEqual(titles({ ...ANSWER, changed: [her, son] }), [
    ["Mit einem Unterschied der Erbkrankheiten", [SON]],
    ["Im Übrigen anders verlaufen", [HER]],
  ]);
  assert.deepEqual(titles({ ...ANSWER, changed: [] }), []);
});
