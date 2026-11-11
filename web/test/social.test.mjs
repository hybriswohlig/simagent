// The social lines of the house page (ADR-0030 sections 3 and 4, ADR-0031 section 8, C24).
// The card shows a belief, the chronicle tells what was said and, for a refutation, the ADR's
// sentence, and "Warum?" says both what the person believes and what is true. A lie states the
// false claim only in the developer view. Written by hand from the words of `text.ts`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { followed } from "../dist/chronicle.js";
import { dayLine } from "../dist/day.js";
import {
  beliefSentence,
  bondSentence,
  companyWords,
  memorySentence,
  refuteSegments,
  speak,
  toldSegments,
  topicPhrase,
} from "../dist/social.js";

const NAMES = new Map([
  [1, "Tom"],
  [2, "Lena"],
  [3, "Mia"],
  [4, "Kind"],
  [5, "Eva"],
]);
const name = (id) => NAMES.get(id) ?? `Person ${id}`;
const view = (sim, cheat = false) => ({ sim, name, cheat });

const father = (man) => ({ vater_von: { child: 4, man } });
const deed = (doer, kind, other) => ({ tat: { doer, kind, other, day: 3 } });
const belief = (statement, extra = {}) => ({
  statement,
  certainty: 900,
  basis: "gehört",
  source: 2,
  origin: 2,
  secret: false,
  since: { t: 1, date: DATE },
  ...extra,
});

const DATE = { year: 1, day_of_year: 0, month: 0, day_of_month: 0, week: 0, weekday: 0, season: 0, second_of_day: 0 };
const entry = (code, fields, subjects = [{ sim: 1 }]) => ({
  entry: { id: 1, t: 1, subjects, causes: [], payload: { [code]: fields } },
  date: DATE,
});

test("a belief on the card, a father once paternity exists, a deed, a source and a secret", () => {
  // ADR-0031 section 8, with "seit" from plan section 2 in the page's date style.
  // "Glaubt seit …: {Tom} ist der Vater von {Kind}. Sicherheit 900, geschlossen."
  const seit = "Glaubt seit Jahr 1, 1. Monat, 1. Tag: ";
  assert.equal(
    beliefSentence(belief(father(1), { basis: "geschlossen", source: 0, origin: 0 }), name),
    `${seit}Tom ist der Vater von Kind. Sicherheit 900, geschlossen.`,
  );
  // The page's example with a source: "Sicherheit 900, gehört, von Lena."
  assert.equal(
    beliefSentence(belief(father(1)), name),
    `${seit}Tom ist der Vater von Kind. Sicherheit 900, gehört, von Lena.`,
  );
  assert.equal(
    beliefSentence(belief(deed(3, 0, 1), { certainty: 800, basis: "gesehen", source: 0 }), name),
    `${seit}Mia hat Tom geneckt. Sicherheit 800, gesehen.`,
  );
  const seen = { year: 1, day_of_year: 57, month: 2, day_of_month: 1, week: 8, weekday: 1, season: 0, second_of_day: 0 };
  assert.equal(
    beliefSentence(belief(deed(3, 0, 1), { certainty: 1000, basis: "gesehen", source: 0, since: { t: 2, date: seen } }), name),
    "Glaubt seit Jahr 1, 3. Monat, 2. Tag: Mia hat Tom geneckt. Sicherheit 1000, gesehen.",
  );
  assert.equal(
    beliefSentence(belief(deed(1, 1, 5), { basis: "beteiligt", source: 0 }), name),
    `${seit}Tom hat über Eva gelästert. Sicherheit 900, beteiligt.`,
  );
  assert.equal(
    beliefSentence(belief(deed(1, 2, 3), { basis: "widerlegt", certainty: 0, source: 0 }), name),
    `${seit}Tom hat Mia belogen. Sicherheit 0, widerlegt.`,
  );
  const secret = beliefSentence(belief(father(1), { secret: true }), name);
  assert.equal(secret, `${seit}Tom ist der Vater von Kind. Sicherheit 900, gehört, von Lena. Geheimnis.`);
  assert.equal(secret.includes("Wahr"), false);
});

test("the six values of one other person, and a memory with its strength and its date", () => {
  const bond = { other: 3, affection: 700, trust: 700, respect: 500, attraction: 500, familiarity: 599, grudge: 0 };
  assert.equal(bondSentence(bond, name), "Mia: Zuneigung 700, Vertrauen 700, Respekt 500, Anziehung 500, Vertrautheit 599, Groll 0");
  const at = { t: 10, date: DATE };
  assert.equal(
    memorySentence({ kind: 0, with: [3], at, strength: 600 }, name),
    "Gespräch mit Mia, Stärke 600, Jahr 1, 1. Monat, 1. Tag.",
  );
  assert.equal(
    memorySentence({ kind: 1, with: [2, 4], at, strength: 900 }, name),
    "Geburt eines Kindes mit Lena und Kind, Stärke 900, Jahr 1, 1. Monat, 1. Tag.",
  );
  assert.equal(
    memorySentence({ kind: 5, with: [], at, strength: 700 }, name),
    "Widerlegung, Stärke 700, Jahr 1, 1. Monat, 1. Tag.",
  );
});

test("a talk on the day list names the partner and the topic", () => {
  const topics = [
    "über den Alltag",
    "über ein Lob",
    "über das Necken",
    "über den Klatsch",
    "über eine Erinnerung",
    "über eine Entschuldigung",
  ];
  topics.forEach((phrase, topic) => assert.equal(topicPhrase(topic), phrase));
  const talked = entry("core:talked", { other: 3, object: 0, topic: 0, actor: null }, [{ sim: 1 }, { sim: 3 }]);
  assert.deepEqual(dayLine({ rooms: [], doors: [], objects: [] }, talked, view(1)), {
    text: "redet mit Mia über den Alltag",
    choice: false,
  });
  // The same entry on the partner's day names the first of the pair.
  assert.equal(dayLine({ rooms: [], doors: [], objects: [] }, talked, view(3)).text, "redet mit Tom über den Alltag");
});

test("the day list says a telling, a lie, a belief, a memory and a bond", () => {
  const layout = { rooms: [], doors: [], objects: [] };
  const told = entry("core:told", { to: 3, statement: deed(5, 1, 1), certainty: 733, lie: false });
  assert.equal(dayLine(layout, told, view(1)).text, "erzählt Mia: Eva hat über Tom gelästert.");
  const lie = entry("core:told", { to: 3, statement: deed(5, 1, 1), certainty: 900, lie: true });
  assert.equal(dayLine(layout, lie, view(1)).text, "erzählt Mia etwas");
  assert.equal(dayLine(layout, lie, view(1)).text.includes("gelästert"), false);
  assert.equal(dayLine(layout, lie, view(1, true)).text, "lügt Mia an: Eva hat über Tom gelästert.");
  const held = entry("core:believed", { statement: deed(3, 0, 1), certainty: 1000, basis: "gesehen", source: null, origin: null, replaced: false });
  assert.equal(dayLine(layout, held, view(1)).text, "glaubt: Mia hat Tom geneckt. Sicherheit 1000, gesehen.");
  const dropped = entry("core:unbelieved", { statement: father(1) });
  assert.equal(dayLine(layout, dropped, view(1)).text, "glaubt es nicht mehr: Tom ist der Vater von Kind.");
  const remembered = entry("core:remembered", { kind: 0, with: [3, null], at: 10, strength: 600 });
  assert.equal(dayLine(layout, remembered, view(1)).text, "erinnert sich: Gespräch mit Mia, Stärke 600.");
  const forgot = entry("core:forgot", { kind: 0, with: [3, null], at: 10, reason: "faded" });
  assert.equal(dayLine(layout, forgot, view(1)).text, "vergisst: Gespräch mit Mia, verblasst.");
  assert.equal(dayLine(layout, entry("core:forgot", { kind: 2, with: [null, null], at: 10, reason: "displaced" }), view(1)).text, "vergisst: Tod eines Verwandten, verdrängt.");
  assert.equal(dayLine(layout, entry("core:bond_formed", { other: 3, values: [400, 450, 400, 0, 200, 0] }), view(1)).text, "knüpft eine Beziehung zu Mia.");
  assert.equal(dayLine(layout, entry("core:bond_changed", { other: 3, before: [], after: [] }), view(1)).text, "ändert die Beziehung zu Mia.");
  assert.equal(dayLine(layout, entry("core:bond_dropped", { other: 3 }), view(1)).text, "löst die Beziehung zu Mia.");
});

test("the chronicle tells a saying, hides a lie, and refutes with the ADR's sentence", () => {
  const told = entry("core:told", { to: 3, statement: deed(5, 1, 1), certainty: 733, lie: false });
  assert.equal(speak(toldSegments(told, false), name), "Tom erzählt Mia: Eva hat über Tom gelästert.");
  const lie = entry("core:told", { to: 3, statement: deed(5, 1, 1), certainty: 900, lie: true });
  assert.equal(speak(toldSegments(lie, false), name), "Tom erzählt Mia etwas");
  assert.equal(speak(toldSegments(lie, true), name), "Tom lügt Mia an: Eva hat über Tom gelästert.");
  // ADR-0031 section 8, corrected 2026-11-11: "{A} widerlegt {B}: {F} ist der Vater von {Kind}."
  // {F} is the man on the corrected statement, not the refuter.
  const parents = entry("core:refuted", { by: 2, statement: father(5), source: 1 }, [{ sim: 1 }]);
  assert.equal(speak(refuteSegments(parents), name), "Lena widerlegt Tom: Eva ist der Vater von Kind.");
  // "{A} widerlegt {B}: {T} hat nicht über {O} gelästert." and the same for the other deeds.
  const gossip = entry("core:refuted", { by: 2, statement: deed(3, 1, 5), source: 1 }, [{ sim: 1 }]);
  assert.equal(speak(refuteSegments(gossip), name), "Lena widerlegt Tom: Mia hat nicht über Eva gelästert.");
  const tease = entry("core:refuted", { by: 2, statement: deed(3, 0, 1), source: null }, [{ sim: 1 }]);
  assert.equal(speak(refuteSegments(tease), name), "Lena widerlegt Tom: Mia hat Tom nicht geneckt.");
  const lied = entry("core:refuted", { by: 2, statement: deed(3, 2, 1), source: null }, [{ sim: 1 }]);
  assert.equal(speak(refuteSegments(lied), name), "Lena widerlegt Tom: Mia hat Tom nicht belogen.");
  assert.equal(dayLine({ rooms: [], doors: [], objects: [] }, gossip, view(1)).text, "Lena widerlegt Tom: Mia hat nicht über Eva gelästert.");
});

test("Warum names the company and, for someone avoided, the belief and the truth", () => {
  const layout = {
    rooms: [],
    doors: [],
    objects: [{ kind: "core:dining_table", room: "kitchen", x: 0, y: 0, w: 1, h: 1, slots: 4, offers: ["eat"] }],
  };
  const emptyBelief = { found: false, statement: {}, certainty: 0, basis: "", source: 0, origin: 0, secret: false, truth: "none" };
  const rows = [
    {
      object: 0,
      liked: 3,
      affection: 640,
      familiarity: 300,
      company: 8000,
      avoided: 1,
      trust: 320,
      avoid: 8000,
      belief: { found: true, statement: deed(3, 0, 1), certainty: 800, basis: "gesehen", source: 0, origin: 0, secret: false, truth: "false" },
    },
    { object: 0, liked: 3, affection: 700, familiarity: 599, company: 8000, avoided: 0, trust: 0, avoid: 0, belief: { ...emptyBelief, found: true, truth: "true" } },
  ];
  assert.equal(
    companyWords([rows[0]], layout, name),
    "setzt sich zu Mia (Zuneigung 640, Vertrautheit 300). meidet den Esstisch: vertraut Tom 320. Glaubt: Mia hat Tom geneckt. Sicherheit 800, gesehen. Wahr ist es nicht.",
  );
  // A liked person alone carries no belief and no truth, even if the row brought one.
  assert.equal(companyWords([rows[1]], layout, name), "setzt sich zu Mia (Zuneigung 700, Vertrautheit 599).");
  const unknown = { ...rows[0], belief: { ...rows[0].belief, truth: "unknown" } };
  assert.equal(companyWords([unknown], layout, name).endsWith("Ob es wahr ist, steht nicht fest."), true);
  const none = { ...rows[0], belief: emptyBelief };
  const avoided = companyWords([none], layout, name);
  assert.equal(avoided.includes("Glaubt"), false);
  assert.equal(avoided.includes("Wahr"), false);
  assert.equal(companyWords([{ ...rows[0], belief: { ...rows[0].belief, truth: "true" } }], layout, name).endsWith("Wahr ist es."), true);
});

test("the chronicle keeps a telling and a refutation, and not every bond change", () => {
  const item = (code) => ({ entry: { id: 1, t: 1, subjects: [{ sim: 1 }], causes: [], payload: { [code]: {} } }, date: DATE });
  assert.equal(followed(item("core:told")), true);
  assert.equal(followed(item("core:refuted")), true);
  assert.equal(followed(item("core:talked")), false);
  assert.equal(followed(item("core:bond_changed")), false);
  assert.equal(followed(item("core:believed")), false);
});
