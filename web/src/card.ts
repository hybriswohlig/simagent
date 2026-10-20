import type { HeightCard, Percentile, TraitCard } from "./generated/export.js";
import { TEXT } from "./text.js";

export type Sex = "female" | "male";

/** The words of a trait's percentile, up and down (Grok L2). */
const WORDS: { readonly [trait: string]: readonly [string, string] } = {
  "core:growth": [TEXT.card_taller, TEXT.card_shorter],
  "core:cognition": [TEXT.card_higher, TEXT.card_lower],
  "core:curl": [TEXT.card_curlier, TEXT.card_straighter],
};

/** The heading of a trait of the card (G3). */
export const TRAIT_HEADINGS: { readonly [trait: string]: string } = {
  "core:growth": TEXT.card_growth,
  "core:cognition": TEXT.card_cognition,
  "core:curl": TEXT.card_curl,
};

function compared(trait: string, percentile: Percentile): string | null {
  const words = WORDS[trait];
  return words === undefined ? null : `${percentile.up ? words[0] : words[1]} ${TEXT.card_than} ${percentile.n} ${TEXT.card_of_100}`;
}

/** The percentile sentence of a trait (Grok G2, L1 to L3): adults against men or women; a
 * child's target height as what it will be; a child's cognition and curls against children of
 * the same age. */
export function traitSentence(row: TraitCard, sex: Sex): string | null {
  const head = compared(row.trait, row.percentile);
  if (head === null) {
    return null;
  }
  const grown = sex === "female" ? TEXT.card_women : TEXT.card_men;
  if (row.adult) {
    return `${head} ${grown} ${TEXT.card_model}`;
  }
  if (row.trait === "core:growth") {
    return `${TEXT.card_will} ${head} ${grown} ${TEXT.card_model}`;
  }
  const young = sex === "female" ? TEXT.card_girls : TEXT.card_boys;
  return `${head} ${young} ${TEXT.card_same_age} ${TEXT.card_model}`;
}

/** The parents' mean (Grok L5), without sex; none without both parents in the run. */
export function parentsMeanSentence(row: TraitCard): string | null {
  const head = row.parents_mean === null ? null : compared(row.trait, row.parents_mean);
  return head === null ? null : `${TEXT.card_parents_mean}: ${head} ${TEXT.card_persons} ${TEXT.card_model}`;
}

const TWO = new Intl.NumberFormat("de-DE", { minimumFractionDigits: 2, maximumFractionDigits: 2, useGrouping: false });
const ONE = new Intl.NumberFormat("de-DE", { minimumFractionDigits: 1, maximumFractionDigits: 1, useGrouping: false });

/** A number with the typographic minus. */
function minus(text: string): string {
  return `−${text}`;
}

/** A part with its sign and two decimals (Grok M2): "+0,42", "−0,13", zero as "0,00". */
export function signed(value: number): string {
  // `-0 === 0`: a negative zero (√0 times a negative draw) is written as zero.
  const text = TWO.format(value < 0 ? -value : value === 0 ? 0 : value);
  if (text === TWO.format(0)) {
    return text;
  }
  return value < 0 ? minus(text) : `+${text}`;
}

export type PartRow = { readonly label: string; readonly value: number; readonly text: string };

/** `value` as an object of the core (keys to values), or null for anything else. */
export function object(value: unknown): { readonly [key: string]: unknown } | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as { readonly [key: string]: unknown })
    : null;
}

/** The rows of the parts of `z` (Grok G5, L4), from the core's object keyed by `kind`; null
 * for an object of another shape. */
export function partRows(value: unknown): { rows: PartRow[]; note: string | null } | null {
  const parts = object(value);
  if (parts === null) {
    return null;
  }
  const number = (key: string): number | null => {
    const value = parts[key];
    return typeof value === "number" ? value : null;
  };
  const rows = (pairs: readonly (readonly [string, string])[]): PartRow[] | null => {
    const found: PartRow[] = [];
    for (const [key, label] of pairs) {
      const value = number(key);
      if (value === null) {
        return null;
      }
      found.push({ label, value, text: signed(value) });
    }
    return found;
  };
  const shared = [
    ["childhood", TEXT.card_childhood],
    ["own", TEXT.card_own],
  ] as const;
  if (parts.kind === "split") {
    const found = rows([["parents", TEXT.card_parents_share], ["passing", TEXT.card_passing], ...shared]);
    return found === null ? null : { rows: found, note: null };
  }
  if (parts.kind === "inherited") {
    const found = rows([["inherited", TEXT.card_inherited], ...shared]);
    const count = number("parents_in_town");
    if (found === null || (count !== 0 && count !== 1)) {
      return null;
    }
    return { rows: found, note: count === 0 ? TEXT.card_no_parents : TEXT.card_one_parent };
  }
  return null;
}

/** The lines in cm under the height (Grok L5, L6): the height now from the body, the losses
 * from hunger years and age when not 0,0 at one decimal, the parents' mean target height. */
export function heightLines(height: HeightCard, nowCm: number): string[] {
  const lines = [`${TEXT.card_height_now}: ${ONE.format(nowCm)} ${TEXT.card_cm}`];
  for (const [label, loss] of [
    [TEXT.card_hunger, height.hunger_cm],
    [TEXT.card_age, height.age_cm],
  ] as const) {
    const text = ONE.format(loss);
    if (text !== ONE.format(0)) {
      lines.push(`${label}: ${minus(text)} ${TEXT.card_cm}`);
    }
  }
  if (height.parents_target_cm !== null) {
    lines.push(`${TEXT.card_parents_target}: ${ONE.format(height.parents_target_cm)} ${TEXT.card_cm}`);
  }
  return lines;
}
