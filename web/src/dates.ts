import type { CalendarDate } from "./generated/export.js";
import { DAYS, MONTHS, TEXT } from "./text.js";

/** The date as the core gives it, its units named by the texts. */
export function dateText(date: CalendarDate): string {
  return `${TEXT.year} ${date.year}, ${MONTHS[date.month] ?? ""}, ${DAYS[date.day_of_month] ?? ""}`;
}
