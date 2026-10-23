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

/** The chronicle after a page of the log: the followed entries of the page it does not
 * keep yet, with the kept, at most `KEPT`, the newest. Pages may come in any order (a
 * window and the entries of an input read beside it). The newest second comes first; the
 * entries of one second keep the core's order, cause before consequence (also across
 * pages). */
export function added(kept: readonly EventItem[], page: readonly EventItem[]): EventItem[] {
  const known = new Set(kept.map((item) => item.entry.id));
  const fresh = page.filter((item) => !known.has(item.entry.id) && followed(item));
  return [...fresh, ...kept]
    .sort((a, b) => b.entry.id - a.entry.id)
    .slice(0, KEPT)
    .sort((a, b) => b.entry.t - a.entry.t || a.entry.id - b.entry.id);
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
