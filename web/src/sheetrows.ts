import type { BodyParams } from "./generated/body.js";
import type { SheetAnswer } from "./generated/export.js";
import { TEXT } from "./text.js";

/** A row of the sheet: its label, its sex and a portrait per age the core gives, with its
 * height. */
export type SheetRow = {
  label: string;
  sex: SheetAnswer["rows"][number]["sex"];
  cells: { age: number; title: string; height: string; body: BodyParams }[];
};

const VARIANTS: Readonly<Record<string, string>> = {
  early: TEXT.sheet_early,
  base: TEXT.sheet_base,
  late: TEXT.sheet_late,
  red: TEXT.sheet_red,
  light_iris: TEXT.sheet_light_iris,
};

/** The column of an age: "2 Jahre" (Grok's round 38, T5). */
export function yearsText(age: number): string {
  return TEXT.sheet_years.replace("{n}", String(age));
}

/** Whole centimetres, from a half up (the format's rounding, "halfExpand"). */
const CENTIMETRES = new Intl.NumberFormat("de-DE", { maximumFractionDigits: 0, useGrouping: false });

/** A height under a portrait: its whole centimetres (K10 of the third playtest, Grok's
 * decision: maturity shows at 12 and 16 years above all in the height). */
export function heightText(cm: number): string {
  return TEXT.sheet_height.replace("{n}", CENTIMETRES.format(cm));
}

/** The rows of the sheet of portraits (ADR-0021 G10; texts Grok's round 38): each founder
 * in the order of the answer, "{variant}, weiblich" or "männlich", its bodies at the ages it
 * reached; every value from the answer. */
export function sheetRows(answer: SheetAnswer): SheetRow[] {
  return answer.rows.map((row) => {
    const label = `${VARIANTS[row.variant] ?? row.variant}, ${row.sex === "female" ? TEXT.female : TEXT.male}`;
    return {
      label,
      sex: row.sex,
      cells: row.at_ages.map((shown) => ({
        age: shown.at.age_years,
        title: `${label}, ${yearsText(shown.at.age_years)}`,
        height: heightText(shown.body.height_cm),
        body: shown.body,
      })),
    };
  });
}
