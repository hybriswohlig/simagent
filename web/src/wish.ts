import type { WishShown } from "./generated/export.js";
import { TEXT } from "./text.js";

/** The sentences of the wish for children on a person page (ADR-0017, addendum of
 * 2026-11-05), as the core decides them: the drawn wish of the person, then, for its open
 * partnership, the first sentence that applies; none while she is pregnant (the pregnancy
 * has its own line). `{0}` is the woman of the couple, `{1}` the man; a grown child is one of
 * the age the server names, the town's (`grown_at`). `name` gives the name
 * of a sim. Below the market age the core sends no wish, and there are none. */
export function wishLines(person: string, shown: WishShown, name: (sim: number) => string): string[] {
  if (shown.wish === null) {
    return [];
  }
  const n = shown.wish;
  const own = n === 0 ? TEXT.wish_none : n === 1 ? TEXT.wish_one : TEXT.wish_many.replace("{n}", String(n));
  const lines = [own.replace("{0}", person)];
  const couple = shown.couple;
  if (couple === null) {
    return lines;
  }
  const fill = (text: string) => text.replaceAll("{0}", name(couple.woman)).replaceAll("{1}", name(couple.man));
  switch (couple.state) {
    case "fertility_over":
      lines.push(fill(TEXT.wish_fertility_over));
      break;
    case "pregnant":
      break;
    case "pause":
      lines.push(fill(TEXT.wish_pause));
      break;
    case "hers":
      lines.push(fill(TEXT.wish_trying));
      break;
    case "his":
      lines.push(fill(`${TEXT.wish_trying} ${TEXT.wish_his}`));
      break;
    case "grown":
      lines.push(fill(`${TEXT.wish_trying} ${TEXT.wish_grown.replace("{age}", String(couple.grown_at))}`));
      break;
    case "fulfilled":
      lines.push(fill(TEXT.wish_fulfilled));
      break;
  }
  return lines;
}
