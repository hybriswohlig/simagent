import type { CalendarDate } from "./generated/export.js";
import { DAYS, MONTHS, TEXT } from "./text.js";

/** The date as the core gives it, its units named by the texts. */
export function dateText(date: CalendarDate): string {
  return `${TEXT.year} ${date.year}, ${MONTHS[date.month] ?? ""}, ${DAYS[date.day_of_month] ?? ""}`;
}

/** Where a life began: in a year from the founding on, else in which year before it (the
 * second playtest: an arrival from the pool read "geboren im Jahr -21"). */
export function bornText(year: number): string {
  if (year >= 0) {
    return `${TEXT.born_in} ${year}`;
  }
  return year === -1 ? TEXT.born_before_one : TEXT.born_before.replace("{n}", String(-year));
}

/** The year under a portrait: the year from the founding on, else that it was before it. */
export function captionYear(year: number): string {
  return year >= 0 ? String(year) : TEXT.before_founding;
}
