import { object } from "./card.js";
import { onsetTemplate } from "./carddiseases.js";
import type { EventItem } from "./generated/export.js";
import { CODES, TEXT } from "./text.js";

/** The payloads a player follows in the chronicle, by their codes; the rest of the log
 * (birthdays, conceptions, meioses) stays in the log. */
export const FOLLOWED: ReadonlySet<string> = new Set([
  "core:started",
  "core:founder_added",
  "core:arrived",
  "core:birth",
  "core:death",
  "core:partnered",
  "core:separated",
  "core:household_formed",
  "core:moved",
  "core:emigrated",
  "core:param_changed",
  "core:disease_onset",
  "core:shock_triggered",
]);

/** The most lines the chronicle keeps; older ones go. */
export const KEPT = 200;

/** The code of an entry's payload: its one key, if it has exactly one. */
export function payloadCode(item: EventItem): string | null {
  const codes = Object.keys(item.entry.payload);
  return codes.length === 1 ? (codes[0] ?? null) : null;
}

/** Whether a player follows `item`: its code is followed, and it is no hunger year another
 * entry caused, whose years the sentence of that entry (the shock) names (the addendum to
 * ADR-0023 before S2, Grok round 27). */
export function followed(item: EventItem): boolean {
  const code = payloadCode(item);
  if (code === null || !FOLLOWED.has(code)) {
    return false;
  }
  const change = object(object(item.entry.payload[code])?.["change"]);
  const caused = item.entry.causes.some((cause) => "event" in cause);
  return !(caused && change !== null && "core:nutrition_year" in change);
}

/** The template of a line (ADR-0023 "Texte" and its addendum before S2): a shock the sentence
 * of its template with the years of its entry, the switch the sentence of its new state, an
 * onset its own (`onsetTemplate`), every other entry the template of its code. */
export function template(item: EventItem): string {
  const code = payloadCode(item) ?? "";
  const value = item.entry.payload[code];
  const payload = object(value);
  if (code === "core:disease_onset") {
    return onsetTemplate(value, item.date.year) ?? TEXT.unknown;
  }
  const [shock, first, last] = [payload?.["shock"], payload?.["first_year"], payload?.["last_year"]];
  const own = code === "core:shock_triggered" && typeof shock === "string" ? CODES[`shock ${shock}`] : undefined;
  if (own !== undefined && typeof first === "number" && typeof last === "number") {
    return own.replace("{first}", String(first)).replace("{last}", String(last));
  }
  const on = code === "core:param_changed" ? object(payload?.["change"])?.["core:hereditary_diseases"] : undefined;
  if (typeof on === "boolean") {
    return on ? TEXT.diseases_now_on : TEXT.diseases_now_off;
  }
  return CODES[`payload ${code}`] ?? "";
}

/** The household number of an entry's payload, if it has one. */
function household(item: EventItem): number | null {
  const value = object(item.entry.payload[payloadCode(item) ?? ""])?.["household"];
  return typeof value === "number" ? value : null;
}

/** What set an entry off (its first cause), as text to compare. */
function trigger(item: EventItem): string {
  return JSON.stringify(item.entry.causes[0] ?? null);
}

/** Whether a move among `lines` says what a line beside it says already (the second
 * playtest, "viele Umzugszeilen"): the child's into its mother's household at its birth,
 * set off by what set off the birth, and a member's into a household formed in that
 * second, whose line names it. Any other move says something of its own (Grok's
 * cross-check): an orphan's to its carer, also in the second of its birth, and a founder
 * child's taken in at the start by a household whose line names only the couple. */
function repeats(lines: readonly EventItem[]): (item: EventItem) => boolean {
  const born = new Set<string>();
  const formed = new Set<string>();
  for (const line of lines) {
    const code = payloadCode(line);
    const child = line.entry.subjects[0];
    if (code === "core:birth" && child !== undefined) {
      born.add(`${line.entry.t} ${child.sim} ${trigger(line)}`);
    }
    const into = household(line);
    if (code === "core:household_formed" && into !== null) {
      for (const member of line.entry.subjects) {
        formed.add(`${line.entry.t} ${into} ${member.sim}`);
      }
    }
  }
  return (item) => {
    const sim = item.entry.subjects[0];
    const into = household(item);
    return (
      payloadCode(item) === "core:moved" &&
      sim !== undefined &&
      (born.has(`${item.entry.t} ${sim.sim} ${trigger(item)}`) ||
        (into !== null && formed.has(`${item.entry.t} ${into} ${sim.sim}`)))
    );
  };
}

/** The chronicle after a page of the log: the followed entries of the page it does not
 * keep yet, with the kept, without the moves that repeat a line beside them, at most
 * `KEPT`, the newest. Pages may come in any order (a window and the entries of an input
 * read beside it), so a move goes also when its birth or household comes later. Newest
 * first, also within one second (Grok's R1): the log writes an entry after every entry of
 * an earlier second, so the larger id is the newer, and the last input of a stop stands on
 * top of its second. */
export function added(kept: readonly EventItem[], page: readonly EventItem[]): EventItem[] {
  const known = new Set(kept.map((item) => item.entry.id));
  const fresh = page.filter((item) => !known.has(item.entry.id) && followed(item));
  const lines = [...fresh, ...kept];
  const repeated = repeats(lines);
  return lines
    .filter((item) => !repeated(item))
    .sort((a, b) => b.entry.id - a.entry.id)
    .slice(0, KEPT);
}

/** A piece of a chronicle line: text, a person by sim, or `null` for a role nobody fills. */
export type Segment = string | number | null;

/**
 * A template of `CODES` with the persons of an entry in their roles (the order of the
 * entry's subjects, which the core gives per payload): `{0}`, `{1}`, `{2}` a role, `{all}`
 * all of them as a list ("a, b und c"). The words between come from `and` and `comma`.
 */
export function segments(template: string, sims: readonly number[], and = " und ", comma = ", "): Segment[] {
  const out: Segment[] = [];
  for (const part of template.split(/(\{all\}|\{[0-9]\})/)) {
    if (part === "{all}") {
      sims.forEach((sim, i) => {
        if (i > 0) {
          out.push(i === sims.length - 1 ? and : comma);
        }
        out.push(sim);
      });
    } else if (/^\{[0-9]\}$/.test(part)) {
      out.push(sims[Number(part.slice(1, 2))] ?? null);
    } else if (part !== "") {
      out.push(part);
    }
  }
  return out;
}
