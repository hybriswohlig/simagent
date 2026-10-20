import type { Kinship } from "./generated/export.js";
import { CODES, TEXT } from "./text.js";

/** `r` as the core rounded it, with its one decimal in German digits. */
const PERCENT = new Intl.NumberFormat("de-DE", {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
  useGrouping: false,
});

/** What the other sim is to a person, as one line (Grok's G6 and K3): the blood word, the
 * foster word beside it, and `r` in percent; without a blood relation the foster word
 * alone, or "nicht verwandt". The words are the core's codes. */
export function kinshipText(kinship: Kinship): string {
  const text = (code: string) => CODES[code] ?? code;
  const related = kinship.word !== "unrelated";
  const parts: string[] = [];
  if (related || kinship.foster === null) {
    parts.push(text(`kinship ${kinship.word}`));
  }
  if (kinship.foster !== null) {
    parts.push(text(`foster ${kinship.foster}`));
  }
  if (related) {
    parts.push(`${PERCENT.format(kinship.r_percent)} ${TEXT.percent}`);
  }
  return parts.join(", ");
}
