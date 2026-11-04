import { objectName, placedOfferWords, whereObject } from "./command.js";
import { dateText } from "./dates.js";
import type { CalendarDate, EventItem, HouseLayout } from "./generated/export.js";
import { wholeMinutes } from "./render/plan.js";
import { CODES, FINISHED_WORDS, KIND_WORDS, ROOM_WORDS, TEXT, WEEKDAYS } from "./text.js";

const NEED_KEYS: { readonly [need: string]: keyof typeof TEXT } = {
  hunger: "need_hunger",
  energy: "need_energy",
  bladder: "need_bladder",
  hygiene: "need_hygiene",
  fun: "need_fun",
};

/** A line of a person's day: its text, and whether it is a choice with a "Warum?". */
export type DayLine = { readonly text: string; readonly choice: boolean };

/** The words of a need by its code. */
function needWords(code: unknown): string {
  const need = NEED_KEYS[String(code)];
  return need === undefined ? String(code) : TEXT[need];
}

/** The end of an action as a line (the playtest of 2026-11-04): what ended, done or full as the
 * offer ends ("isst fertig am Esstisch"), cut off with why ("hört auf: isst am Esstisch
 * (Befehl)"); a sleep "wacht auf". */
function ended(layout: HouseLayout, object: number, offer: number, reason: unknown): string {
  const drawn = layout.objects[object];
  const name = drawn?.offers[offer] ?? "";
  const sleep = name === "sleep";
  if (reason === "done" || reason === "full") {
    if (sleep) {
      return TEXT.house_woke;
    }
    const finished = FINISHED_WORDS[name];
    const at = `${KIND_WORDS[drawn?.kind ?? ""]?.[1] ?? drawn?.kind ?? ""}${whereObject(layout, object)}`;
    if (finished !== undefined) {
      return TEXT.house_end_done.replace("{finished}", finished).replace("{at}", at);
    }
  }
  const codes: Record<string, string> = CODES;
  const why = codes[`end_reason ${String(reason)}`] ?? String(reason);
  return sleep
    ? TEXT.house_woken.replace("{why}", why)
    : TEXT.house_end_broken.replace("{what}", placedOfferWords(layout, object, offer)).replace("{why}", why);
}

/** The line of a house entry of a person's day, or null for one the day leaves out (an
 * arrival): what it chose or was told, what ended, where it idles or what it waits for, a need
 * critical or empty, the day's duty, a collapse and an accident; the object told apart from
 * others of its kind, all from the entry and the layout. */
export function dayLine(layout: HouseLayout, item: EventItem): DayLine | null {
  const [code, body] = Object.entries(item.entry.payload)[0] ?? ["", {}];
  const fields = body as { [key: string]: unknown };
  const number = (key: string) => (typeof fields[key] === "number" ? (fields[key] as number) : 0);
  const school = fields["place"] === "school";
  switch (code) {
    case "core:action_started": {
      // A commanded start says so (ADR-0024 section 5); "Warum?" tells the points beside it.
      const said = fields["commanded"] === true ? TEXT.house_commanded : TEXT.house_choice;
      return { text: said.replace("{what}", placedOfferWords(layout, number("object"), number("offer"))), choice: true };
    }
    case "core:action_ended":
      return { text: ended(layout, number("object"), number("offer"), fields["reason"]), choice: false };
    case "core:idle": {
      const waits = fields["waits"];
      if (typeof waits === "number") {
        return { text: TEXT.house_waits.replace("{object}", objectName(layout, waits)), choice: true };
      }
      const room = layout.rooms[number("room")]?.id ?? "";
      return { text: TEXT.house_idle_day.replace("{room}", ROOM_WORDS[room]?.[2] ?? room), choice: true };
    }
    case "core:need_critical":
      return {
        text: TEXT.house_critical.replace("{need}", needWords(fields["need"])).replace("{level}", String(number("level"))),
        choice: false,
      };
    // The day's duty (ADR-0024 section 4): where, when; leaving, late; back; missed.
    case "core:day_planned":
      return { text: school ? TEXT.house_planned_school : TEXT.house_planned_work, choice: false };
    case "core:left_home": {
      const minutes = wholeMinutes(number("late_secs"));
      const late = minutes > 0 ? ` ${TEXT.house_late.replace("{minutes}", String(minutes))}` : "";
      return { text: `${school ? TEXT.house_left_school : TEXT.house_left_work}${late}`, choice: false };
    }
    case "core:came_home":
      return { text: school ? TEXT.house_came_school : TEXT.house_came_work, choice: false };
    case "core:missed":
      return { text: school ? TEXT.house_missed_school : TEXT.house_missed_work, choice: false };
    // A need at 0 and what it brings (ADR-0026, addendum of 2026-11-04 on a need at 0).
    case "core:need_empty":
      return { text: TEXT.house_empty.replace("{need}", needWords(fields["need"])), choice: false };
    case "core:collapsed":
      return { text: TEXT.house_collapsed, choice: false };
    case "core:accident":
      return { text: TEXT.house_accident, choice: false };
    default:
      return null;
  }
}

/** The entries of a day list by calendar day, in their order: each day with the date of its
 * first entry, which names it ("Dienstag, Jahr 20, 3. Monat, 5. Tag"). */
export function dayGroups<T extends { readonly date: CalendarDate }>(
  items: readonly T[],
): { readonly date: CalendarDate; readonly title: string; readonly items: readonly T[] }[] {
  const groups: { date: CalendarDate; title: string; items: T[] }[] = [];
  for (const item of items) {
    const last = groups[groups.length - 1];
    if (last !== undefined && last.date.year === item.date.year && last.date.day_of_year === item.date.day_of_year) {
      last.items.push(item);
    } else {
      const title = TEXT.house_day_of.replace("{weekday}", WEEKDAYS[item.date.weekday] ?? "").replace("{date}", dateText(item.date));
      groups.push({ date: item.date, title, items: [item] });
    }
  }
  return groups;
}
