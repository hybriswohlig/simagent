import { object } from "./card.js";
import { dateText } from "./dates.js";
import type { EventItem, HouseBond, HouseLayout, HouseMemory, WhyCompany } from "./generated/export.js";
import { KIND_WORDS, TEXT } from "./text.js";

/** A piece of a social line: text, a person by sim, or `null` when a role is empty. */
export type Segment = string | number | null;

/** A name for a sim, already resolved by the page. */
export type NameOf = (id: number) => string;

/** Who a day line is about, how to name the others, and whether the developer view is on. */
export type DayView = { readonly sim: number; readonly name: NameOf; readonly cheat: boolean };

/** The words of a belief the card and "Warum?" share (ADR-0031 section 8). `source` is 0 when
 * the belief has none. `VaterVon` is worded here so the line works once paternity exists. */
export type BeliefWords = {
  readonly statement: { readonly [key: string]: unknown };
  readonly certainty: number;
  readonly basis: string;
  readonly source: number;
  readonly secret: boolean;
};

const TOPICS: readonly (keyof typeof TEXT)[] = [
  "social_topic_everyday",
  "social_topic_praise",
  "social_topic_tease",
  "social_topic_gossip",
  "social_topic_recall",
  "social_topic_apology",
];

const KINDS: readonly (keyof typeof TEXT)[] = [
  "social_kind_talk",
  "social_kind_born",
  "social_kind_died",
  "social_kind_partnered",
  "social_kind_separated",
  "social_kind_refuted",
];

function fill(text: string, values: { readonly [key: string]: string | number }): string {
  return text.replace(/\{([a-z_]+)\}/g, (whole, key: string) => {
    const value = values[key];
    return value === undefined ? whole : String(value);
  });
}

function num(value: unknown): number | null {
  return typeof value === "number" ? value : null;
}

/** The persons in a log field, dropping the empty slots of a pair. The order is the server's. */
function personIds(value: unknown): number[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const ids: number[] = [];
  for (const one of value) {
    if (typeof one === "number" && one > 0) {
      ids.push(one);
    }
  }
  return ids;
}

function subject(item: EventItem, index: number): number | null {
  return item.entry.subjects[index]?.sim ?? null;
}

function bodyOf(item: EventItem): { readonly code: string; readonly body: { readonly [key: string]: unknown } } | null {
  const code = Object.keys(item.entry.payload)[0];
  if (code === undefined) {
    return null;
  }
  const body = object(item.entry.payload[code]);
  return body === null ? null : { code, body };
}

/** A name from the page's map, else "Person" and the number. */
export function named(names: ReadonlyMap<number, string>): NameOf {
  return (id) => names.get(id) ?? `${TEXT.person} ${id}`;
}

/** The words of a line's pieces, each person by `name`. */
export function speak(parts: readonly Segment[], name: NameOf): string {
  return parts.map((part) => (part === null ? TEXT.unknown : typeof part === "number" ? name(part) : part)).join("");
}

/** Names joined as the chronicle joins them: "Mia", "Mia und Tom", "Mia, Tom und Eva". */
export function joinNames(ids: readonly number[], name: NameOf): string {
  const words = ids.map((id) => name(id));
  if (words.length <= 1) {
    return words[0] ?? "";
  }
  const last = words[words.length - 1] ?? "";
  return `${words.slice(0, -1).join(", ")} ${TEXT.and} ${last}`;
}

/** The topic of a talk, in the order of the social data. Empty when the number is none of them. */
export function topicPhrase(topic: number): string {
  const key = TOPICS[topic];
  return key === undefined ? "" : TEXT[key];
}

function memoryKind(kind: number): string {
  const key = KINDS[kind];
  return key === undefined ? TEXT.social_kind_other : TEXT[key];
}

function memoryWith(kind: number, withIds: readonly number[], name: NameOf): string {
  const people = joinNames(withIds, name);
  const withText = people === "" ? "" : fill(TEXT.social_with, { names: people });
  return `${memoryKind(kind)}${withText}`;
}

function memoryCore(kind: number, withIds: readonly number[], strength: number, name: NameOf): string {
  return fill(TEXT.social_memory_now, { kind: memoryWith(kind, withIds, name), with: "", strength });
}

/** One memory on the card: its kind, whom it is with, its strength and its date, all from the
 * server (ADR-0030 section 1). */
export function memorySentence(memory: HouseMemory, name: NameOf): string {
  const people = joinNames(memory.with, name);
  const withText = people === "" ? "" : fill(TEXT.social_with, { names: people });
  return fill(TEXT.social_memory, { kind: memoryKind(memory.kind), with: withText, strength: memory.strength, date: dateText(memory.at.date) });
}

/** The six values of a bond, in the order the server settles them. */
export function bondValues(bond: HouseBond): string {
  return fill(TEXT.social_bond_values, {
    affection: bond.affection,
    trust: bond.trust,
    respect: bond.respect,
    attraction: bond.attraction,
    familiarity: bond.familiarity,
    grudge: bond.grudge,
  });
}

/** A bond row as text: the other person, then the six values. */
export function bondSentence(bond: HouseBond, name: NameOf): string {
  return `${name(bond.other)}: ${bondValues(bond)}`;
}

/** A deed or a father, as pieces so each person stays a link. `father` replaces the man of a
 * `VaterVon` (the refutation names the refuter, ADR-0031 section 8). `negate` is the refutation
 * of a deed. The house day stays out of the sentence. */
function claim(statement: unknown, negate: boolean, father: number | null): Segment[] | null {
  const body = object(statement);
  if (body === null) {
    return null;
  }
  const fatherOf = object(body["vater_von"]);
  if (fatherOf !== null) {
    const child = num(fatherOf["child"]);
    const man = father ?? num(fatherOf["man"]);
    if (child === null || man === null) {
      return null;
    }
    return [man, ` ${TEXT.social_father} `, child];
  }
  const deed = object(body["tat"]);
  if (deed === null) {
    return null;
  }
  const doer = num(deed["doer"]);
  const other = num(deed["other"]);
  const kind = num(deed["kind"]);
  if (doer === null || other === null || kind === null) {
    return null;
  }
  if (kind === 1) {
    const mid = negate ? TEXT.social_has_not_about : TEXT.social_has_about;
    return [doer, ` ${mid} `, other, ` ${TEXT.social_gossip_verb}`];
  }
  if (kind === 0 || kind === 2) {
    const verb = kind === 2 ? TEXT.social_lied_verb : TEXT.social_teased_verb;
    const tail = negate ? ` ${TEXT.social_not} ${verb}` : ` ${verb}`;
    return [doer, " hat ", other, tail];
  }
  return null;
}

function statementText(statement: unknown, name: NameOf): string {
  const parts = claim(statement, false, null);
  return parts === null ? TEXT.social_something : speak(parts, name);
}

function beliefTail(belief: BeliefWords, name: NameOf): { readonly statement: string; readonly source: string } {
  const source = belief.source > 0 ? fill(TEXT.social_belief_source, { name: name(belief.source) }) : "";
  return { statement: statementText(belief.statement, name), source };
}

/** The card's line for one belief (ADR-0031 section 8). The card shows the belief, not whether
 * it is true. A secret hangs on the end. */
export function beliefSentence(belief: BeliefWords, name: NameOf): string {
  const tail = beliefTail(belief, name);
  const line = fill(TEXT.social_belief, { statement: tail.statement, certainty: belief.certainty, basis: belief.basis, source: tail.source });
  return belief.secret ? `${line}${TEXT.social_secret}` : line;
}

/** What is true of an avoided person's belief, the server's word (ADR-0031 section 5). Empty
 * when the choice found no such belief. */
export function truthSentence(truth: string): string {
  switch (truth) {
    case "true":
      return TEXT.social_true;
    case "false":
      return TEXT.social_false;
    case "unknown":
      return TEXT.social_unknown;
    default:
      return "";
  }
}

function accusative(layout: HouseLayout, index: number): string {
  const kind = layout.objects[index]?.kind ?? "";
  return KIND_WORDS[kind]?.[2] ?? kind;
}

/** "Warum?" for the company of a choice (ADR-0030 section 4, ADR-0031 section 5): who one sits
 * with, whom one avoids, and for an avoided person the belief and whether it is true. The
 * numbers are the server's. Rows stay in the server's order. */
export function companyWords(rows: readonly WhyCompany[], layout: HouseLayout, name: NameOf): string {
  const lines: string[] = [];
  for (const row of rows) {
    if (row.liked !== 0) {
      lines.push(fill(TEXT.social_sits, { name: name(row.liked), affection: row.affection, familiarity: row.familiarity }));
    }
    if (row.avoided !== 0) {
      lines.push(fill(TEXT.social_avoids, { object: accusative(layout, row.object), name: name(row.avoided), trust: row.trust }));
      if (row.belief.found) {
        lines.push(beliefSentence(row.belief, name));
        const truth = truthSentence(row.belief.truth);
        if (truth !== "") {
          lines.push(truth);
        }
      }
    }
  }
  return lines.join(" ");
}

/** A telling, as the chronicle says it (ADR-0031 sections 4 and 8, C24). A lie does not state
 * the false claim unless `cheat` is on; then it says that it is a lie. */
export function toldSegments(item: EventItem, cheat: boolean): Segment[] | null {
  const read = bodyOf(item);
  if (read === null || read.code !== "core:told") {
    return null;
  }
  const hearer = num(read.body["to"]);
  const speaker = subject(item, 0);
  if (hearer === null) {
    return null;
  }
  const lie = read.body["lie"] === true;
  if (lie && !cheat) {
    return [speaker, ` ${TEXT.social_tells} `, hearer, ` ${TEXT.social_something}`];
  }
  const said = claim(read.body["statement"], false, null);
  if (said === null) {
    return [speaker, ` ${TEXT.social_tells} `, hearer, ` ${TEXT.social_something}`];
  }
  const verb = lie ? TEXT.social_lies : TEXT.social_tells;
  const tail = lie ? ` ${TEXT.social_lies_tail} ` : ": ";
  return [speaker, ` ${verb} `, hearer, tail, ...said, "."];
}

/** A refutation, the ADR's sentence (ADR-0031 section 8). For a father the refuter is named as
 * the father; for a deed the deed is denied. */
export function refuteSegments(item: EventItem): Segment[] | null {
  const read = bodyOf(item);
  if (read === null || read.code !== "core:refuted") {
    return null;
  }
  const by = num(read.body["by"]);
  if (by === null) {
    return null;
  }
  const statement = read.body["statement"];
  const father = object(statement)?.["vater_von"] === undefined ? null : by;
  const said = claim(statement, true, father);
  if (said === null) {
    return null;
  }
  return [by, ` ${TEXT.social_refutes} `, subject(item, 0), ": ", ...said, "."];
}

/** The chronicle's own line for a telling or a refutation, or null to keep the generic template. */
export function socialChronicle(item: EventItem, cheat: boolean): Segment[] | null {
  return toldSegments(item, cheat) ?? refuteSegments(item);
}

function talkDay(item: EventItem, body: { readonly [key: string]: unknown }, view: DayView): string | null {
  const other = num(body["other"]);
  if (other === null) {
    return null;
  }
  const first = subject(item, 0);
  const partner = view.sim === other && first !== null && first !== other ? first : other;
  const topic = topicPhrase(num(body["topic"]) ?? -1);
  return fill(TEXT.social_talk, { partner: view.name(partner), topic }).trim();
}

function beliefDay(body: { readonly [key: string]: unknown }, view: DayView, held: boolean): string | null {
  const statement = statementText(body["statement"], view.name);
  if (!held) {
    return fill(TEXT.social_unbelieves, { statement });
  }
  const sourceId = num(body["source"]) ?? 0;
  const source = sourceId > 0 ? fill(TEXT.social_belief_source, { name: view.name(sourceId) }) : "";
  return fill(TEXT.social_believes, {
    statement,
    certainty: num(body["certainty"]) ?? 0,
    basis: typeof body["basis"] === "string" ? body["basis"] : "",
    source,
  });
}

function memoryDay(body: { readonly [key: string]: unknown }, view: DayView, forgotten: boolean): string | null {
  const kind = num(body["kind"]);
  if (kind === null) {
    return null;
  }
  const what = memoryWith(kind, personIds(body["with"]), view.name);
  if (forgotten) {
    const reason = body["reason"] === "displaced" ? TEXT.social_displaced : body["reason"] === "faded" ? TEXT.social_faded : "";
    return reason === "" ? fill(TEXT.social_remembers, { what }) : fill(TEXT.social_forgets, { what, reason });
  }
  const strength = num(body["strength"]);
  if (strength === null) {
    return null;
  }
  return fill(TEXT.social_remembers, { what: memoryCore(kind, personIds(body["with"]), strength, view.name) });
}

function otherDay(body: { readonly [key: string]: unknown }, view: DayView, text: string): string | null {
  const other = num(body["other"]);
  return other === null ? null : fill(text, { name: view.name(other) });
}

/** A social entry of a person's day, or null when the entry is not one of these. The talk names
 * the other person of the pair. A lie follows the chronicle (C24). */
export function daySocial(item: EventItem, view?: DayView): { readonly text: string; readonly choice: false } | null {
  const read = bodyOf(item);
  if (read === null) {
    return null;
  }
  const seen = view ?? { sim: subject(item, 0) ?? 0, name: (id: number) => `${TEXT.person} ${id}`, cheat: false };
  const line = (text: string | null) => (text === null ? null : { text, choice: false as const });
  switch (read.code) {
    case "core:talked":
      return line(talkDay(item, read.body, seen));
    case "core:told": {
      const parts = toldSegments(item, seen.cheat);
      if (parts === null) {
        return null;
      }
      // The day list is already this person's, so the speaker's name, the first piece, goes.
      const rest = typeof parts[0] === "number" ? parts.slice(1) : parts;
      return line(speak(rest, seen.name).trim());
    }
    case "core:refuted": {
      const parts = refuteSegments(item);
      return line(parts === null ? null : speak(parts, seen.name));
    }
    case "core:believed":
      return line(beliefDay(read.body, seen, true));
    case "core:unbelieved":
      return line(beliefDay(read.body, seen, false));
    case "core:remembered":
      return line(memoryDay(read.body, seen, false));
    case "core:forgot":
      return line(memoryDay(read.body, seen, true));
    case "core:bond_formed":
      return line(otherDay(read.body, seen, TEXT.social_bond_formed));
    case "core:bond_changed":
      return line(otherDay(read.body, seen, TEXT.social_bond_changed));
    case "core:bond_dropped":
      return line(otherDay(read.body, seen, TEXT.social_bond_dropped));
    default:
      return null;
  }
}

