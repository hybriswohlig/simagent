import type { HouseLayout } from "./generated/export.js";
import { KIND_WORDS, OFFER_WORDS, ROOM_WORDS, TEXT } from "./text.js";

/** An offer a person can be told to take (ADR-0024 section 5): the object and the offer by
 * the core's numbers, and how the house page names it. */
export type CommandChoice = { readonly object: number; readonly offer: number; readonly label: string };

/** What an offer of an object is, as a person does it: "isst am Esstisch". */
export function offerWords(layout: HouseLayout, object: number, offer: number): string {
  const drawn = layout.objects[object];
  const name = drawn?.offers[offer] ?? "";
  const kind = KIND_WORDS[drawn?.kind ?? ""];
  return TEXT.house_does.replace("{offer}", OFFER_WORDS[name] ?? name).replace("{at}", kind?.[1] ?? drawn?.kind ?? "");
}

/** Every offer of every object of the drawing, by object and then offer, named as a person
 * does it and in which room: "isst am Esstisch in der Küche". The house answer gives no ages,
 * so every offer is there and the core refuses one the person's age does not allow. A name
 * that two offers share carries their order among them: "(1)", "(2)". */
export function commandChoices(layout: HouseLayout): CommandChoice[] {
  const named = layout.objects.flatMap((drawn, object) =>
    drawn.offers.map((_, offer) => {
      const room = ROOM_WORDS[drawn.room]?.[2] ?? drawn.room;
      const label = TEXT.house_command_choice.replace("{what}", offerWords(layout, object, offer)).replace("{room}", room);
      return { object, offer, label };
    }),
  );
  const count = new Map<string, number>();
  for (const choice of named) {
    count.set(choice.label, (count.get(choice.label) ?? 0) + 1);
  }
  const seen = new Map<string, number>();
  return named.map((choice) => {
    if ((count.get(choice.label) ?? 0) < 2) {
      return choice;
    }
    const n = (seen.get(choice.label) ?? 0) + 1;
    seen.set(choice.label, n);
    return { ...choice, label: TEXT.house_command_numbered.replace("{label}", choice.label).replace("{n}", String(n)) };
  });
}
