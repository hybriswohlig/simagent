import type { EventItem } from "./generated/export.js";

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
]);

/** The most lines the chronicle keeps; older ones go. */
export const KEPT = 200;

/** The code of an entry's payload: its one key, if it has exactly one. */
export function payloadCode(item: EventItem): string | null {
  const codes = Object.keys(item.entry.payload);
  return codes.length === 1 ? (codes[0] ?? null) : null;
}

/** The chronicle after a page of the log: the followed entries of the page that are newer
 * than the newest kept, newest first, then the kept, at most `KEPT`. */
export function added(kept: readonly EventItem[], page: readonly EventItem[]): EventItem[] {
  const newest = kept[0]?.entry.id;
  const fresh = page.filter(
    (item) => (newest === undefined || item.entry.id > newest) && FOLLOWED.has(payloadCode(item) ?? ""),
  );
  return [...fresh.reverse(), ...kept].slice(0, KEPT);
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
