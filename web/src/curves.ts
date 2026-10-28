import { UnknownAnswer } from "./api.js";
import { diseaseName } from "./carddiseases.js";
import type { CensusAnswer } from "./generated/export.js";
import { TEXT } from "./text.js";

/** A line of a curve: a value per year, `null` where the census counted nobody, and the
 * last year's value as text. */
export type Line = { name: string; values: (number | null)[]; last: string | null };

/** A picture: its title, whether its axis starts at zero, the decimals it shows, the unit of
 * its axis, its lines, the sentences above it and under its legend, and how a value and a
 * tick of its axis read. */
export type Curve = {
  title: string;
  fromZero: boolean;
  decimals: number;
  unit: string | null;
  lines: Line[];
  note: string | null;
  after: string | null;
  label: (value: number) => string;
  tick: (value: number) => string;
};

/** The pictures over `years`, whether the switch ended the last year off, and the hunger
 * years in the picture with the legend that names them while there are any (Grok round 40). */
export type Curves = { years: number[]; curves: Curve[]; off: boolean; hunger: number[]; hungerLegend: string | null };

/** A year in the table of values, with its mark if it was a hunger year (Grok round 40). */
export function yearLabel(year: number, hunger: readonly number[]): string {
  return hunger.includes(year) ? `${year} (${TEXT.curve_hunger_year})` : String(year);
}

function digits(min: number, max: number): Intl.NumberFormat {
  return new Intl.NumberFormat("de-DE", {
    minimumFractionDigits: min,
    maximumFractionDigits: max,
    useGrouping: false,
  });
}
const DIGITS = [digits(0, 0), digits(1, 1), digits(2, 2)];
const TICKS = [digits(0, 0), digits(0, 1), digits(0, 2)];

/** The lines of the deaths beside the diseases (Grok R4). */
const CAUSES: { readonly [code: string]: string } = {
  "core:senescence": TEXT.curve_senescence,
  "core:accident": TEXT.curve_accident,
};

/** The name of a cause of death: age, accident or the name of the disease (Grok R4). */
export function causeName(code: string): string {
  return CAUSES[code] ?? diseaseName(code);
}

function curve(
  title: string,
  decimals: 0 | 1 | 2,
  fromZero: boolean,
  lines: [string, (number | null)[]][],
  note: string | null = null,
  after: string | null = null,
): Curve {
  const unit = [null, TEXT.card_cm, TEXT.percent][decimals] ?? null;
  const format = (value: number) => (DIGITS[decimals] as Intl.NumberFormat).format(value);
  const label = decimals === 2 ? (value: number) => `${format(value)} ${TEXT.percent}` : format;
  return {
    title,
    fromZero,
    decimals,
    unit,
    lines: lines.map(([name, values]) => {
      const last = values[values.length - 1] ?? null;
      return { name, values, last: last === null ? null : label(last) };
    }),
    note,
    after,
    label,
    tick: (value) => (TICKS[decimals] as Intl.NumberFormat).format(value),
  };
}

/**
 * The curves of a run from its census (ADR-0021, addendum 2026-10-21; Grok R1 to R4, S1).
 * The columns are read by name; a mean is shown in its unit (mm as cm, `e4` as percent) and
 * has no point where its count is 0. The card's sentence stands above the affected and the
 * deaths when the switch is off at the end of the last year. Of the server's `hunger` years
 * those in the picture are kept, and while there are any Grok's sentence stands under the
 * legend of the two pictures of the heights (round 40).
 *
 * Throws `UnknownAnswer` for a census without a column of the curves (the deaths of age and
 * of accidents among them, which the addendum always has) or with a short row.
 */
export function curves(answer: CensusAnswer, hunger: readonly number[]): Curves {
  const index = new Map(answer.columns.map((column, i) => [column, i]));
  const cells = (i: number) =>
    answer.rows.map((row) => {
      const cell = row[i];
      if (cell === undefined || row.length !== answer.columns.length) {
        throw new UnknownAnswer("a row of the census is short");
      }
      return cell;
    });
  const column = (name: string) => {
    const i = index.get(name);
    if (i === undefined) {
      throw new UnknownAnswer(`the census has no column ${name}`);
    }
    return cells(i);
  };
  const byCode = (prefix: string, name: (code: string) => string): [string, number[]][] =>
    answer.columns.flatMap((column, i): [string, number[]][] =>
      column.startsWith(prefix) ? [[name(column.slice(prefix.length)), cells(i)]] : [],
    );
  const mean = (count: string, value: string, scale: number) => {
    const counts = column(count);
    return column(value).map((cell, i) => (counts[i] === 0 ? null : cell / scale));
  };
  const height = (group: string, mm: string): [string, (number | null)[]][] => [
    [TEXT.curve_women, mean(`${group}_female`, `${mm}_female`, 10)],
    [TEXT.curve_men, mean(`${group}_male`, `${mm}_male`, 10)],
  ];
  const years = column("year");
  for (const cause of Object.keys(CAUSES)) {
    column(`deaths.${cause}`);
  }
  const on = column("diseases_on");
  const off = on.length > 0 && on[on.length - 1] === 0;
  const note = off ? TEXT.card_diseases_off : null;
  const shown = years.filter((year) => hunger.includes(year));
  const after = shown.length > 0 ? TEXT.curve_hunger_note : null;
  return {
    years,
    off,
    hunger: shown,
    hungerLegend: shown.length > 0 ? TEXT.curve_hunger_years : null,
    curves: [
      curve(TEXT.living, 0, true, [[TEXT.living, column("living")]]),
      curve(TEXT.curve_moves, 0, true, [
        [TEXT.curve_births, column("births")],
        [TEXT.curve_arrivals, column("arrivals")],
        [TEXT.curve_emigrations, column("emigrations")],
      ]),
      curve(TEXT.curve_deaths, 0, true, byCode("deaths.", causeName), note),
      curve(TEXT.curve_affected, 0, true, byCode("affected.", diseaseName), note),
      curve(TEXT.curve_adults, 1, false, height("adults", "adult_height_mm"), null, after),
      curve(TEXT.curve_age25, 1, false, height("age25", "age25_height_mm"), null, after),
      curve(TEXT.curve_inbreeding, 2, true, [[TEXT.curve_inbreeding, mean("births", "inbreeding_e4", 100)]]),
    ],
  };
}
