import { TEXT } from "./text.js";

/** The mood in the points of an offer of "Warum?", the server's number in percent with its sign
 * (ADR-0026, addendum of 2026-11-04 on the mood): " (Laune +6 %)", " (Laune −3 %)", zero as
 * " (Laune 0 %)"; nothing where there is none, for a command or a closed offer. */
export function moodWords(mood: number | null): string {
  if (mood === null) {
    return "";
  }
  const signed = mood > 0 ? `+${mood}` : mood < 0 ? `−${-mood}` : "0";
  return TEXT.house_why_mood.replace("{mood}", signed);
}
