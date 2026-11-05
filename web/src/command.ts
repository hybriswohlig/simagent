import type { HouseAction, HouseLayout, HouseOffer } from "./generated/export.js";
import { KIND_WORDS, OFFER_WORDS, ROOM_WORDS, TEXT } from "./text.js";

/** An offer a person can be told to take (ADR-0024 section 5): the object and the offer by
 * the core's numbers, how the house page names it, and whether its object has no free place
 * for the person, as the server says. */
export type CommandChoice = { readonly object: number; readonly offer: number; readonly label: string; readonly taken: boolean };

/** Where object `index` of the drawing is, as its name needs it to be told apart: nothing if
 * the house has no other of its kind; else " im Bad", the room the layout gives, and where that
 * room has more of the kind, its order among them there: " im Kinderzimmer (2)" (the
 * playtest of 2026-11-04: two toilets, three single beds). */
export function whereObject(layout: HouseLayout, index: number): string {
  const drawn = layout.objects[index];
  if (drawn === undefined) {
    return "";
  }
  const kin = layout.objects.filter((other) => other.kind === drawn.kind);
  if (kin.length < 2) {
    return "";
  }
  const room = ` ${ROOM_WORDS[drawn.room]?.[2] ?? drawn.room}`;
  const here = layout.objects.flatMap((other, i) => (other.kind === drawn.kind && other.room === drawn.room ? [i] : []));
  return here.length < 2 ? room : TEXT.house_command_numbered.replace("{label}", room).replace("{n}", String(here.indexOf(index) + 1));
}

/** An object's name, told apart from others of its kind: "Toilette im Bad", "Dusche". */
export function objectName(layout: HouseLayout, index: number): string {
  const kind = layout.objects[index]?.kind ?? "";
  return `${KIND_WORDS[kind]?.[0] ?? kind}${whereObject(layout, index)}`;
}

/** An object as one waits for it, told apart from others of its kind: "die Toilette im Bad",
 * "die Dusche". */
export function objectWaitedFor(layout: HouseLayout, index: number): string {
  const kind = layout.objects[index]?.kind ?? "";
  return `${KIND_WORDS[kind]?.[2] ?? kind}${whereObject(layout, index)}`;
}

/** What an offer of an object is, as a person does it: "isst am Esstisch". */
export function offerWords(layout: HouseLayout, object: number, offer: number): string {
  const drawn = layout.objects[object];
  const name = drawn?.offers[offer] ?? "";
  const kind = KIND_WORDS[drawn?.kind ?? ""];
  return TEXT.house_does.replace("{offer}", OFFER_WORDS[name] ?? name).replace("{at}", kind?.[1] ?? drawn?.kind ?? "");
}

/** What an offer is, told apart from the same offer of another object of its kind: "ist auf
 * der Toilette im WC", "schläft im Einzelbett im Kinderzimmer (1)", "isst am Esstisch". */
export function placedOfferWords(layout: HouseLayout, object: number, offer: number): string {
  return `${offerWords(layout, object, offer)}${whereObject(layout, object)}`;
}

/** The offers of the drawing a command may name, by object and then offer, named as a person
 * does it and in which room: "isst am Esstisch in der Küche". Only those `allowed` lists, as
 * the house answer gives them per person (version 5: open at its age, sleep only in its night);
 * every offer without it. A name that two offers of the drawing share carries their order among
 * them, whether both are allowed or not: "(1)", "(2)". An offer whose object has no free place
 * for the person, as the house answer says (version 8), is taken and says so, "(besetzt)"; else
 * the offer the person does now, `doing`, says that: "(schon dabei)" (ADR-0021, addendum of
 * 2026-11-05 on the house page). */
export function commandChoices(layout: HouseLayout, allowed?: readonly HouseOffer[], doing?: HouseAction | null): CommandChoice[] {
  const named = layout.objects.flatMap((drawn, object) =>
    drawn.offers.map((_, offer) => {
      const room = ROOM_WORDS[drawn.room]?.[2] ?? drawn.room;
      const label = TEXT.house_command_choice.replace("{what}", offerWords(layout, object, offer)).replace("{room}", room);
      return { object, offer, label, taken: false };
    }),
  );
  const count = new Map<string, number>();
  for (const choice of named) {
    count.set(choice.label, (count.get(choice.label) ?? 0) + 1);
  }
  const seen = new Map<string, number>();
  const numbered = named.map((choice) => {
    if ((count.get(choice.label) ?? 0) < 2) {
      return choice;
    }
    const n = (seen.get(choice.label) ?? 0) + 1;
    seen.set(choice.label, n);
    return { ...choice, label: TEXT.house_command_numbered.replace("{label}", choice.label).replace("{n}", String(n)) };
  });
  if (allowed === undefined) {
    return numbered;
  }
  return numbered.flatMap((choice) => {
    const open = allowed.find((each) => each.object === choice.object && each.offer === choice.offer);
    if (open === undefined) {
      return [];
    }
    const own = doing !== undefined && doing !== null && doing.object === choice.object && doing.offer === choice.offer;
    const mark = open.taken ? ` ${TEXT.house_command_taken}` : own ? ` ${TEXT.house_command_own}` : "";
    return [{ ...choice, label: `${choice.label}${mark}`, taken: open.taken }];
  });
}
