// The chronicle of a run page keeps the entries a player follows, newest first, also within
// one second (Grok's R1, reversing the core's order of Q1 for the page), each once, at most a
// fixed number (ADR-0021 section 2: the log in the order of the file).
import { test } from "node:test";
import assert from "node:assert/strict";

import { onsetTemplate } from "../dist/carddiseases.js";
import { KEPT, added, payloadCode, segments, template } from "../dist/chronicle.js";
import { CODES } from "../dist/text.js";

const DATE = {
  year: 1,
  day_of_year: 2,
  month: 0,
  day_of_month: 2,
  week: 0,
  weekday: 2,
  season: 0,
  second_of_day: 3,
};

/** An item of a page; the values are arbitrary, only the shape follows the format. */
function item(id, code) {
  return {
    entry: { id, t: id, subjects: [{ sim: 1 }], causes: [{ event: 1 }], payload: { [code]: {} } },
    date: DATE,
  };
}

test("an entry's code is the one key of its payload", () => {
  assert.equal(payloadCode(item(1, "core:birth")), "core:birth");
  const two = { ...item(2, "core:birth"), entry: { ...item(2, "x").entry, payload: { a: {}, b: {} } } };
  assert.equal(payloadCode(two), null);
});

test("only followed payloads are kept, newest first", () => {
  const page = [
    item(1, "core:birth"),
    item(2, "core:birthday"),
    item(3, "core:meiosis"),
    item(4, "core:death"),
    item(5, "core:conception"),
    item(6, "core:partnered"),
  ];
  const kept = added([], page);
  assert.deepEqual(
    kept.map((i) => i.entry.id),
    [6, 4, 1],
  );
});

test("a page that overlaps the kept adds each entry once", () => {
  const kept = added([], [item(1, "core:birth"), item(2, "core:death")]);
  const next = added(kept, [item(2, "core:death"), item(3, "core:moved")]);
  assert.deepEqual(
    next.map((i) => i.entry.id),
    [3, 2, 1],
  );
});

/** An item of the second `t` about `sim`. */
function during(id, t, code, sim) {
  return { entry: { id, t, subjects: [{ sim }], causes: [], payload: { [code]: {} } }, date: DATE };
}

// Grok's Q1 and R1: at a year start a family arrives with a disease already begun (sim 100),
// and a resident's planned onset comes in the same second (sim 1). The core writes the
// arrival, its onset, then the resident's; the chronicle shows them newest first, the
// resident's onset on top, the arrival below its consequence.
test("one second shows its newest entry first", () => {
  const page = [
    during(1, 5, "core:birth", 7),
    during(2, 9, "core:arrived", 100),
    during(3, 9, "core:disease_onset", 100),
    during(4, 9, "core:disease_onset", 1),
  ];
  assert.deepEqual(
    added([], page).map((i) => i.entry.id),
    [4, 3, 2, 1],
  );
});

test("a second split over two pages is newest first too", () => {
  const kept = added([], [during(1, 5, "core:birth", 7), during(2, 9, "core:arrived", 100)]);
  const next = added(kept, [
    during(2, 9, "core:arrived", 100),
    during(3, 9, "core:disease_onset", 100),
    during(4, 9, "core:disease_onset", 1),
    during(5, 12, "core:death", 7),
  ]);
  assert.deepEqual(
    next.map((i) => i.entry.id),
    [5, 4, 3, 2, 1],
  );
});

// The playtest (N2, F11): inputs at one stop act in the next second in the order they came.
// A hunger after a pairing stands above the pairing's lines, and the switch turned off and on
// again shows "on" on top, the state it has.
test("the last input of a second stands on top of it", () => {
  const page = [
    during(20, 60, "core:partnered", 5),
    during(21, 60, "core:household_formed", 5),
    during(22, 60, "core:moved", 5),
    during(23, 60, "core:shock_triggered", 0),
    during(24, 61, "core:param_changed", 0),
    during(25, 61, "core:param_changed", 0),
  ];
  assert.deepEqual(
    added([], page).map((i) => i.entry.id),
    [25, 24, 23, 22, 21, 20],
  );
});

// Astra's review of S2: the entries of an input, read beside a window, may come before the
// window's older ones; those still come in.
test("a page of older entries that comes later still adds them", () => {
  const kept = added([], [during(9, 20, "core:shock_triggered", 1)]);
  const next = added(kept, [during(7, 18, "core:birth", 3), during(8, 19, "core:death", 4)]);
  assert.deepEqual(
    next.map((i) => i.entry.id),
    [9, 8, 7],
  );
});

test("the chronicle keeps at most its limit, the newest", () => {
  const page = Array.from({ length: KEPT + 5 }, (_, i) => item(i + 1, "core:birth"));
  const kept = added([], page);
  assert.equal(kept.length, KEPT);
  assert.equal(kept[0]?.entry.id, KEPT + 5);
  assert.equal(kept.at(-1)?.entry.id, 6);
});

test("a template puts the persons in their roles, all members as a list", () => {
  assert.deepEqual(segments("{0} wird geboren, Mutter {1}, Vater {2}", [9, 4, 5]), [
    9,
    " wird geboren, Mutter ",
    4,
    ", Vater ",
    5,
  ]);
  assert.deepEqual(segments("Neuer Haushalt: {all}", [1]), ["Neuer Haushalt: ", 1]);
  assert.deepEqual(segments("Neuer Haushalt: {all}", [1, 2]), ["Neuer Haushalt: ", 1, " und ", 2]);
  assert.deepEqual(segments("Neuer Haushalt: {all}", [1, 2, 3]), [
    "Neuer Haushalt: ",
    1,
    ", ",
    2,
    " und ",
    3,
  ]);
  assert.deepEqual(segments("Die Welt beginnt.", []), ["Die Welt beginnt."]);
  assert.deepEqual(segments("{0} und {1}", [7]), [7, " und ", null], "a role without a person");
});

// The onset of a disease (Grok P1, P2, P3): the short form in the year of the onset, else the
// late form with the year and the whole years ("seit der Geburt" at 0, "mit 1 Jahr" at 1).
// Every expected sentence is written by hand from Grok's wording; the name is G1's, uninflected.
test("the chronicle follows the onset of a disease", () => {
  const page = [item(1, "core:disease_onset"), item(2, "core:birthday")];
  assert.deepEqual(
    added([], page).map((i) => i.entry.id),
    [1],
  );
});

test("an onset in its own year is the short form, the name in the dative after „an“ (Grok round 41, V3)", () => {
  const onset = (disease, since_year, age_years) => ({ disease, since_year, age_years });
  assert.equal(onsetTemplate(onset("core:disease_ar", 12, 0), 12), "{0} erkrankt an Zähem Atem");
  assert.equal(onsetTemplate(onset("core:disease_ad_late", 40, 45), 40), "{0} erkrankt an Spätem Verfall");
  assert.equal(onsetTemplate(onset("core:disease_mt", -3, 21), -3), "{0} erkrankt an Mütterlichem Augenlicht");
  assert.equal(onsetTemplate(onset("core:disease_new", 7, 30), 7), "{0} erkrankt an core:disease_new");
});

test("an onset written later is the late form, the name in the dative too", () => {
  const onset = (disease, since_year, age_years) => ({ disease, since_year, age_years });
  assert.equal(
    onsetTemplate(onset("core:disease_ar", 12, 12), 20),
    "{0} ist an Zähem Atem erkrankt, seit Jahr 12, mit 12 Jahren",
  );
  assert.equal(onsetTemplate(onset("core:disease_xr", -10, 0), 4), "{0} ist an Schwerer Blutung erkrankt, seit der Geburt");
  assert.equal(
    onsetTemplate(onset("core:disease_liability", 5, 1), 9),
    "{0} ist an Geteilter Schwere erkrankt, seit Jahr 5, mit 1 Jahr",
  );
  assert.equal(
    onsetTemplate(onset("core:disease_ad_partial", -10, 30), 0),
    "{0} ist an Ungleichem Ausbruch erkrankt, seit Jahr -10, mit 30 Jahren",
  );
});

test("an onset of another shape has no template", () => {
  assert.equal(onsetTemplate(null, 3), null);
  assert.equal(onsetTemplate({ disease: "core:disease_ar", since_year: 3 }, 3), null);
  assert.equal(onsetTemplate({ disease: 4, since_year: 3, age_years: 0 }, 3), null);
});

/** An entry of the second `id` with `payload` and `causes`, about nobody. */
function about(id, payload, causes) {
  return { entry: { id, t: id, subjects: [], causes, payload }, date: DATE };
}
const shock = (id, shock, first_year, last_year) =>
  about(id, { "core:shock_triggered": { shock, first_year, last_year } }, [{ input: 2 }]);
const changed = (id, change, causes) => about(id, { "core:param_changed": { change } }, causes);
const hunger = (year) => ({ "core:nutrition_year": { year, index: 0.7 } });
const diseases = (on) => ({ "core:hereditary_diseases": on });

// ADR-0023 "Texte" (Grok rounds 23 and 26) and the addendum before S2: the sentence of a
// shock's template with the years of its entry, the generic one for a template the page does
// not know; the sentence of the switch's new state. Written by hand from Grok's wording.
test("a shock names the years of its entry", () => {
  assert.equal(template(shock(1, "core:hunger_one_year", 29, 29)), "Ein Hungerjahr kommt: Jahr 29.");
  assert.equal(template(shock(1, "core:hunger_three_years", 29, 31)), "Drei Hungerjahre kommen: Jahre 29 bis 31.");
  assert.equal(template(shock(1, "core:hunger_three_years", -2, 0)), "Drei Hungerjahre kommen: Jahre -2 bis 0.");
  assert.equal(template(shock(1, "mod:flood", 3, 4)), "Ein Schock kommt.");
  assert.equal(template(about(1, { "core:shock_triggered": { shock: "core:hunger_one_year" } }, [])), "Ein Schock kommt.");
});

test("the switch says how it stands from now on", () => {
  assert.equal(template(changed(1, diseases(false), [{ input: 2 }])), "Die Erbkrankheiten sind von nun an ausgeschaltet.");
  assert.equal(template(changed(1, diseases(true), [{ input: 2 }])), "Die Erbkrankheiten sind von nun an angeschaltet.");
  assert.equal(template(changed(1, hunger(3), [{ input: 2 }])), "Eine Weltregel ändert sich.");
});

test("every other entry keeps the template of its code", () => {
  assert.equal(template(item(1, "core:birth")), CODES["payload core:birth"]);
  const onset = about(1, { "core:disease_onset": { disease: "core:disease_ar", since_year: 1, age_years: 0 } }, []);
  assert.equal(template(onset), "{0} erkrankt an Zähem Atem");
});

// Grok round 27: the years a shock sets stand in its sentence, not each on its own line; a
// hunger year of a fork's override (caused by the input) keeps its line.
test("the chronicle follows a shock, not the years it sets", () => {
  const page = [
    shock(7, "core:hunger_three_years", 1, 3),
    changed(8, hunger(1), [{ event: 7 }]),
    changed(9, hunger(2), [{ event: 7 }]),
    changed(10, hunger(5), [{ input: 3 }]),
    changed(11, diseases(true), [{ input: 4 }]),
  ];
  assert.deepEqual(
    added([], page).map((i) => i.entry.id),
    [11, 10, 7],
  );
});

/** An entry of the second `t` about `sims` with `payload`, set off by `trigger` (its first
 * cause) under `rule`, in the shape the core writes. */
function at(id, t, sims, payload, trigger, rule) {
  const causes = [trigger, { rule }];
  return { entry: { id, t, subjects: sims.map((sim) => ({ sim })), causes, payload }, date: DATE };
}
const plan = (by) => ({ plan: { by, planned_at: 0 } });
const birth = (id, t, child, mother, father) =>
  at(id, t, [child, mother, father], { "core:birth": { lineage_key: "a" } }, plan(id), "core:gestation");
const moved = (id, t, sim, household, trigger) => at(id, t, [sim], { "core:moved": { household } }, trigger, "core:household");
const formed = (id, t, sims, household, trigger) =>
  at(id, t, sims, { "core:household_formed": { household } }, trigger, "core:household");
const ids = (items) => items.map((i) => i.entry.id);

// The second playtest ("viele Umzugszeilen"): a move a line beside it says already is not
// shown, the child's into its mother's household at its birth (set off by what set off the
// birth) and a member's into a household formed in that second, whose line names it. A move
// into a household that was there before (an orphan to its carer) says something of its own.
test("a move a line beside it says already is not shown", () => {
  const page = [
    birth(1, 50, 9, 3, 4),
    moved(2, 50, 9, 21, plan(1)),
    at(3, 60, [5, 6], { "core:partnered": { partnership: 7 } }, plan(2), "core:partner_market"),
    formed(4, 60, [5, 6], 22, { event: 3 }),
    moved(5, 60, 5, 22, { event: 3 }),
    moved(6, 60, 6, 22, { event: 3 }),
    at(7, 70, [3], { "core:death": { cause: "core:senescence" } }, plan(3), "core:mortality"),
    moved(8, 70, 9, 22, { event: 7 }),
  ];
  assert.deepEqual(ids(added([], page)), [8, 7, 4, 3, 1]);
});

test("a move in another second than its birth or its household's, or of another, stays", () => {
  const page = [birth(1, 50, 9, 3, 4), formed(2, 50, [3], 21, plan(5)), moved(3, 51, 9, 21, plan(6)), moved(4, 50, 8, 23, plan(7))];
  assert.deepEqual(ids(added([], page)), [4, 3, 2, 1]);
});

// Grok's cross-check: a child that moves again in the second of its birth for another
// reason (an orphan placed after a death) and a founder child taken in at the start by a
// household formed in that second, whose line names only the couple, keep their lines.
test("a move set off otherwise, or by a household that does not name the mover, stays", () => {
  const page = [
    birth(1, 50, 9, 3, 4),
    moved(2, 50, 9, 21, plan(1)),
    at(3, 50, [3], { "core:death": { cause: "core:senescence" } }, plan(8), "core:mortality"),
    moved(4, 50, 9, 30, { event: 3 }),
    formed(5, 0, [6, 7], 31, { input: 1 }),
    moved(6, 0, 8, 31, { input: 1 }),
  ];
  assert.deepEqual(ids(added([], page)), [6, 5, 4, 3, 1]);
});

test("a move goes also when its birth or its household comes in a later page", () => {
  const kept = added([], [moved(2, 50, 9, 21, plan(1)), moved(5, 60, 5, 22, { event: 3 })]);
  assert.deepEqual(ids(kept), [5, 2]);
  assert.deepEqual(ids(added(kept, [birth(1, 50, 9, 3, 4), formed(4, 60, [5, 6], 22, { event: 3 })])), [4, 1]);
});

test("the limit counts the lines shown", () => {
  const page = Array.from({ length: KEPT }, (_, i) => [
    birth(2 * i + 1, i, 1000 + i, 3, 4),
    moved(2 * i + 2, i, 1000 + i, 21, plan(2 * i + 1)),
  ]).flat();
  const kept = added([], page);
  assert.equal(kept.length, KEPT);
  assert.ok(kept.every((i) => payloadCode(i) === "core:birth"));
});
