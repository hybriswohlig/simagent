import { causeName } from "./curves.js";
import { dateText } from "./dates.js";
import type { DatedSecond, DiffAnswer, DiffBond, DiffLife } from "./generated/export.js";
import type { Named } from "./names.js";
import { TEXT } from "./text.js";

/** A person of one run only: its key, its name and its sim in that run. */
export type Who = { key: string; name: string; sim: number | null };
/** A field that differs, with the lines of each run. */
export type Row = { label: string; a: string[]; b: string[] };
/** A life that went otherwise: its name, its sim in each run and its rows. */
export type Otherwise = { key: string; name: string; a: number | null; b: number | null; rows: Row[] };
export type DiffView = { aOnly: Who[]; bOnly: Who[]; changed: Otherwise[] };

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
    default:
      return [name, []];
  }
}

/**
 * The comparison of two runs as the page shows it (ADR-0021 section 3 and its addenda;
 * texts Grok round 28): the persons of one run only, named from that run, and each life that
 * went otherwise with a row per field that differs, in the order of the answer. Every value
 * is the answer's: the page computes nothing from them.
 */
export function diffView(answer: DiffAnswer, names: { a: Names; b: Names }): DiffView {
  const who = (run: Names) => (key: string): Who => ({ key, name: nameOf(key, run), sim: run.get(key)?.sim ?? null });
  return {
    aOnly: answer.a_only.map(who(names.a)),
    bOnly: answer.b_only.map(who(names.b)),
    changed: answer.changed.map((entry) => ({
      key: entry.lineage_key,
      name: nameOf(entry.lineage_key, names.a, names.b),
      a: names.a.get(entry.lineage_key)?.sim ?? null,
      b: names.b.get(entry.lineage_key)?.sim ?? null,
      rows: entry.differs.map((name) => {
        const [label, a] = field(name, entry.a, names.a);
        const [, b] = field(name, entry.b, names.b);
        return { label, a, b };
      }),
    })),
  };
}
