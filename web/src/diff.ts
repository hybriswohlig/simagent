import { atAge, diseaseName } from "./carddiseases.js";
import { causeName } from "./curves.js";
import { dateText } from "./dates.js";
import type { DatedSecond, DiffAnswer, DiffBond, DiffLife, DiffOnset } from "./generated/export.js";
import type { Named } from "./names.js";
import { TEXT } from "./text.js";

/** A person of one run only: its key, its name and its sim in that run. */
export type Who = { key: string; name: string; sim: number | null };
/** A field that differs, with the lines of each run. */
export type Row = { label: string; a: string[]; b: string[] };
/** A life that went otherwise: its name, its sim in each run and its rows. */
export type Otherwise = { key: string; name: string; a: number | null; b: number | null; rows: Row[] };
/** A disease counted in both runs: its name and the cell of each run. */
export type Counted = { name: string; a: string; b: string };
/** The lives that went otherwise under one heading. */
export type Group = { title: string; lives: Otherwise[] };
export type DiffView = { summary: Counted[]; aOnly: Who[]; bOnly: Who[]; changed: Otherwise[]; groups: Group[] };

type Names = ReadonlyMap<string, Named>;

const ENDS: { readonly [end: string]: string } = {
  death: TEXT.diff_end_death,
  separation: TEXT.diff_end_separation,
  emigration: TEXT.diff_end_emigration,
};

function nameOf(key: string, ...names: Names[]): string {
  for (const run of names) {
    const named = run.get(key);
    if (named !== undefined) {
      return named.name;
    }
  }
  return `${TEXT.person} ${key}`;
}

function when(second: DatedSecond | null, none: string): string {
  return second === null ? none : dateText(second.date);
}

/** A partnership as the comparison writes it (Grok V6): partner, start, end and how. */
function bond(bond: DiffBond, names: Names): string {
  const start = TEXT.diff_bond.replace("{name}", nameOf(bond.partner, names)).replace("{from}", dateText(bond.formed.date));
  const until = bond.ended === null ? "" : TEXT.diff_bond_until.replace("{to}", dateText(bond.ended.date));
  const how = bond.ended_by === null ? "" : `, ${ENDS[bond.ended_by] ?? bond.ended_by}`;
  return `${start}${until}${how}`;
}

/** A disease that shows, in the words of card and chronicle (Grok round 34, E3), or one that
 * rests (round 41, V2). */
function onset(shown: DiffOnset): string {
  if (shown.rests) {
    return TEXT.diff_disease_rests.replace("{disease}", diseaseName(shown.disease));
  }
  const template = shown.age_years === 0 ? TEXT.diff_disease_from_birth : TEXT.diff_disease_since;
  return template
    .replace("{disease}", diseaseName(shown.disease))
    .replace("{year}", String(shown.year))
    .replace("{age}", atAge(shown.age_years));
}

/** The label and the lines of a field of a life, by the field's name in `differs`. */
function field(name: string, life: DiffLife, names: Names): [string, string[]] {
  switch (name) {
    case "born":
      return [TEXT.diff_born, [dateText(life.born.date)]];
    case "died":
      return [TEXT.diff_died, [when(life.died, TEXT.diff_no_death)]];
    case "cause_of_death":
      return [TEXT.diff_cause, [life.cause_of_death === null ? TEXT.diff_no_cause : causeName(life.cause_of_death)]];
    case "emigrated":
      return [TEXT.diff_emigrated, [when(life.emigrated, TEXT.diff_no_emigration)]];
    case "partnerships":
      return [
        TEXT.diff_partnerships,
        life.partnerships.length === 0 ? [TEXT.diff_no_partnership] : life.partnerships.map((each) => bond(each, names)),
      ];
    case "children":
      return [TEXT.diff_children, [String(life.children)]];
    case "diseases":
      return [TEXT.diff_diseases, life.diseases.length === 0 ? [TEXT.diff_no_disease] : life.diseases.map(onset)];
    default:
      return [name, []];
  }
}

/** The cell of a disease counted in one run (Grok round 41, V1). */
function counted(numbers: { affected: number; died: number }): string {
  return TEXT.diff_counted.replace("{show}", String(numbers.affected)).replace("{dead}", String(numbers.died));
}

/**
 * The comparison of two runs as the page shows it (ADR-0021 section 3 and its addenda;
 * texts Grok rounds 28, 34 and 41): the hereditary diseases counted per run, the persons of
 * one run only, named from that run, and each life that went otherwise with a row per field
 * that differs, in the order of the answer, and in groups: those the core marks hereditary
 * first. Every value is the answer's: the page computes nothing from them.
 */
export function diffView(answer: DiffAnswer, names: { a: Names; b: Names }): DiffView {
  const who = (run: Names) => (key: string): Who => ({ key, name: nameOf(key, run), sim: run.get(key)?.sim ?? null });
  const changed = answer.changed.map((entry) => ({
      key: entry.lineage_key,
      name: nameOf(entry.lineage_key, names.a, names.b),
      a: names.a.get(entry.lineage_key)?.sim ?? null,
      b: names.b.get(entry.lineage_key)?.sim ?? null,
      rows: entry.differs.map((name) => {
        const [label, a] = field(name, entry.a, names.a);
        const [, b] = field(name, entry.b, names.b);
        return { label, a, b };
      }),
    }));
  const hereditary = changed.filter((_, i) => answer.changed[i]?.hereditary === true);
  const others = changed.filter((_, i) => answer.changed[i]?.hereditary !== true);
  const groups: Group[] = [
    { title: TEXT.diff_changed_hereditary, lives: hereditary },
    { title: hereditary.length === 0 ? TEXT.diff_changed : TEXT.diff_changed_others, lives: others },
  ].filter((group) => group.lives.length > 0);
  return {
    summary: answer.summary.map((row) => ({ name: diseaseName(row.disease), a: counted(row.a), b: counted(row.b) })),
    aOnly: answer.a_only.map(who(names.a)),
    bOnly: answer.b_only.map(who(names.b)),
    changed,
    groups,
  };
}

/** The page a life's name leads to (the third playtest, K8): its person in the first run,
 * the one the comparison was opened from, else in the second; none where neither names it. */
export function home(life: Otherwise, a: string, b: string): { run: string; sim: number } | null {
  if (life.a !== null) {
    return { run: a, sim: life.a };
  }
  return life.b === null ? null : { run: b, sim: life.b };
}

/** A piece of a sentence of the page: text, or a run to link to. */
export type YearPart = string | { run: string };

/** A template with its slots filled, each run a link of its own. */
function linked(template: string, values: { readonly [slot: string]: YearPart }): YearPart[] {
  const parts: YearPart[] = [];
  for (const piece of template.split(/(\{[a-z]+\})/)) {
    const part = values[piece] ?? piece;
    const last = parts.at(-1);
    if (typeof part === "string" && typeof last === "string") {
      parts[parts.length - 1] = last + part;
    } else if (part !== "") {
      parts.push(part);
    }
  }
  return parts;
}

/** Where the two runs stand (Grok W3): the sentence with each run as a link. */
export function years(a: string, x: number, b: string, y: number): YearPart[] {
  return linked(TEXT.diff_years, { "{a}": { run: a }, "{b}": { run: b }, "{x}": String(x), "{y}": String(y) });
}

/** The second the runs are compared at, when one of them was recomputed to it (Grok round
 * 42, D2), or nothing when both stood there already. */
export function comparedAt(answer: Pick<DiffAnswer, "a" | "b" | "at" | "recomputed">): YearPart[] | null {
  const later = answer.recomputed;
  if (later === null) {
    return null;
  }
  const earlier = later === answer.a ? answer.b : answer.a;
  return linked(TEXT.diff_compared_at, {
    "{earlier}": { run: earlier },
    "{later}": { run: later },
    "{date}": dateText(answer.at.date),
  });
}
