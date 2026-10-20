import type { Kinship } from "./generated/export.js";
import { CODES, TEXT } from "./text.js";

/** `r` as the core rounded it, with its one decimal in German digits. */
const PERCENT = new Intl.NumberFormat("de-DE", {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
  useGrouping: false,
});

/** What the other sim is to a person, as one line (Grok's G6, K3 and K5): the blood word,
 * the foster word beside it, and `r` in percent, always (0,0 included); without a blood
 * relation the foster word alone, or "nicht verwandt". The words are the core's codes. */
export function kinshipText(kinship: Kinship): string {
  const text = (code: string) => CODES[code] ?? code;
  const parts: string[] = [];
  if (kinship.word !== "unrelated" || kinship.foster === null) {
    parts.push(text(`kinship ${kinship.word}`));
  }
  if (kinship.foster !== null) {
    parts.push(text(`foster ${kinship.foster}`));
  }
  parts.push(`${PERCENT.format(kinship.r_percent)} ${TEXT.percent}`);
  return parts.join(", ");
}
