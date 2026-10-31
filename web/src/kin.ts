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

/** What a person of the family tree is to its centre, under the name (Grok's round 46, P3):
 * the core's word with its grade and "Halb-", without the generation that the row shows, the
 * foster word after it, then "·" and `r` in percent: "Cousine 1. Grades · 12,5 %",
 * "Tochter, Pflegetochter · 50,0 %", "Pflegemutter · 0,0 %". */
export function kinNodeText(kinship: Kinship): string {
  const text = (code: string) => CODES[code] ?? code;
  const words: string[] = [];
  if (kinship.word !== "unrelated" || kinship.foster === null) {
    words.push(text(`kinship ${kinship.word.replace(/_(older|younger)\d+$/, "")}`));
  }
  if (kinship.foster !== null) {
    words.push(text(`foster ${kinship.foster}`));
  }
  return `${words.join(", ")} · ${PERCENT.format(kinship.r_percent)} %`;
}
