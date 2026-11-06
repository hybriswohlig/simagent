import type { FoundingValue, RunSetting } from "./generated/export.js";
import { TEXT } from "./text.js";

/**
 * Own values when a town is founded (ADR-0021, addendum of 2026-11-05). The server names the
 * values a town may set with the base game's value of each and what the base game's towns
 * show of it; the page only says what each does, compares a field with the base game's value
 * and formats the numbers.
 */

/** The texts of a value the page knows: its label, what it does, what the base game gives
 * with the server's number of it, and how a town with it is marked. */
type Known = {
  label: keyof typeof TEXT;
  what: keyof typeof TEXT;
  gives: keyof typeof TEXT | null;
  mark: keyof typeof TEXT;
};

const KNOWN: Readonly<Record<string, Known>> = {
  "core:market_age": {
    label: "settings_market_age",
    what: "settings_market_age_what",
    gives: null,
    mark: "settings_market_age_mark",
  },
  "core:market_search_center_female": {
    label: "settings_search_center",
    what: "settings_search_center_what",
    gives: "settings_search_center_gives",
    mark: "settings_search_center_mark",
  },
  "core:pairing_age_gap_preferred": {
    label: "settings_age_gap",
    what: "settings_age_gap_what",
    gives: "settings_age_gap_gives",
    mark: "settings_age_gap_mark",
  },
  // The children's wish (ADR-0017, addendum of 2026-11-05).
  "core:fertility_wish_tilt": {
    label: "settings_tilt",
    what: "settings_tilt_what",
    gives: "settings_tilt_gives",
    mark: "settings_tilt_mark",
  },
  "core:fertility_count_until": {
    label: "settings_count_until",
    what: "settings_count_until_what",
    gives: null,
    mark: "settings_count_until_mark",
  },
  "core:fertility_extra": {
    label: "settings_extra",
    what: "settings_extra_what",
    gives: null,
    mark: "settings_extra_mark",
  },
  "core:fertility_grown_from": {
    label: "settings_grown_from",
    what: "settings_grown_from_what",
    gives: null,
    mark: "settings_grown_from_mark",
  },
  "core:fertility_grown_chance": {
    label: "settings_grown_chance",
    what: "settings_grown_chance_what",
    gives: null,
    mark: "settings_grown_chance_mark",
  },
};

const NUMBER = new Intl.NumberFormat("de-DE", { maximumFractionDigits: 4, useGrouping: false });

/** The text of each field, by the value's name. */
export type Fields = Readonly<Record<string, string>>;

/** The fields at the base game's values the server named. */
export function fieldsOf(founding: readonly FoundingValue[]): Record<string, string> {
  return Object.fromEntries(founding.map((value) => [value.key, String(value.base)]));
}

/** The number a field holds, or null if it holds none. */
function numberOf(text: string | undefined): number | null {
  if (text === undefined || text.trim() === "") {
    return null;
  }
  const number = Number(text);
  return Number.isFinite(number) ? number : null;
}

/** The values whose field differs from the base game's value, by name; null while a field
 * holds no number. */
export function changedValues(founding: readonly FoundingValue[], fields: Fields): Record<string, number> | null {
  const changed: Record<string, number> = {};
  for (const value of founding) {
    const number = numberOf(fields[value.key]);
    if (number === null) {
      return null;
    }
    if (number !== value.base) {
      changed[value.key] = number;
    }
  }
  return changed;
}

/** The body of `POST /api/runs`: own values only when there are some. */
export function foundBody(template: string, run: string, changed: Readonly<Record<string, number>>) {
  return Object.keys(changed).length === 0 ? { template, run } : { template, run, settings: changed };
}

/** The label of a value's field; a value the page does not know goes by its name. */
export function labelOf(key: string): string {
  const known = KNOWN[key];
  return known === undefined ? key : TEXT[known.label];
}

/** The line under a field: what the value does and what the base game gives, with the
 * server's numbers. */
export function hintOf(value: FoundingValue): string {
  const known = KNOWN[value.key];
  const base = NUMBER.format(value.base);
  const gives = known === undefined ? null : known.gives;
  const tail =
    gives !== null && value.outcome !== null
      ? TEXT[gives].replace("{base}", base).replace("{outcome}", NUMBER.format(value.outcome))
      : TEXT.settings_base.replace("{base}", base);
  return known === undefined ? tail : `${TEXT[known.what]} ${tail}`;
}

/** The own values of a town in words, or null for a town of the base game. */
export function markOf(settings: readonly RunSetting[]): string | null {
  if (settings.length === 0) {
    return null;
  }
  return settings
    .map(({ key, value }) => {
      const known = KNOWN[key];
      const number = NUMBER.format(value);
      return known === undefined ? `${key}: ${number}` : TEXT[known.mark].replace("{value}", number);
    })
    .join("; ");
}
