import { objectName, placedOfferWords } from "./command.js";
import type { HouseLayout, HouseReserve, WhyAnswer, WhyChoice, WhyOffer } from "./generated/export.js";
import { timeOfDay } from "./render/plan.js";
import { ROOM_WORDS, TEXT } from "./text.js";

/** A mood in percent with its sign, the typographic minus: "+6", "−3", "0". */
function signed(mood: number): string {
  return mood > 0 ? `+${mood}` : mood < 0 ? `−${-mood}` : "0";
}

/** The mood in the points of an offer of "Warum?", the server's number in percent with its sign
 * (ADR-0026, addendum of 2026-11-04 on the mood): " (Laune +6 %)", " (Laune −3 %)", zero as
 * " (Laune 0 %)"; nothing where there is none, for a command or a closed offer. */
export function moodWords(mood: number | null): string {
  if (mood === null) {
    return "";
  }
  return TEXT.house_why_mood.replace("{mood}", signed(mood));
}

/** Points as German numbers read, with the typographic minus: "269.900", "−250". */
export function pointsText(points: number): string {
  return points < 0 ? `−${(-points).toLocaleString("de-DE")}` : points.toLocaleString("de-DE");
}

/** The needs keys of the texts by the core's codes, in the order of its arrays (hunger, energy,
 * bladder, hygiene, fun). */
const NEED_TEXTS: readonly (readonly [string, keyof typeof TEXT])[] = [
  ["hunger", "need_hunger"],
  ["energy", "need_energy"],
  ["bladder", "need_bladder"],
  ["hygiene", "need_hygiene"],
  ["fun", "need_fun"],
];

/** What makes an offer urgent, its critical needs with their values in the choice, from the
 * server: "dringend (Hygiene 50)", several "dringend (Hunger 100, Blase 120)". */
export function urgentWords(urgent: readonly string[], needs: readonly number[]): string {
  const named = urgent.map((code) => {
    const index = NEED_TEXTS.findIndex(([need]) => need === code);
    const key = NEED_TEXTS[index]?.[1];
    const level = needs[index];
    return TEXT.house_why_need
      .replace("{need}", key === undefined ? code : TEXT[key])
      .replace("{level}", level === undefined ? "" : String(level));
  });
  return TEXT.house_why_urgent.replace("{needs}", named.join(", "));
}

/** The worth of an offer as "Warum?" tells it: its points with the mood in them, "269.900
 * Punkte (Laune +8 %)" (in a list without the word, `listed`); an urgent one by what makes it
 * urgent instead of its points of some billions (the playtest of 2026-11-04). */
function score(offer: WhyChoice | WhyOffer, needs: readonly number[], listed: boolean): string {
  if (offer.urgent.length > 0) {
    return urgentWords(offer.urgent, needs);
  }
  const points = offer.points === null ? "" : pointsText(offer.points);
  return (listed ? TEXT.house_why_score : TEXT.house_why_points).replace("{points}", points).replace("{mood}", moodWords(offer.mood));
}

/** The offers of a list of "Warum?", at most `most`, each with its worth and whether its object
 * had no free place, an offer closed by day with the start of the night `opens`, one closed by
 * the departure with its time; an offer whose needs stand at or above their comfort, `comfort`,
 * says so (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler). */
function listed(
  layout: HouseLayout,
  offers: readonly WhyOffer[],
  needs: readonly number[],
  comfort: readonly number[],
  opens: string,
  most: number,
): string {
  return offers
    .slice(0, most)
    .map((offer) => {
      const what = placedOfferWords(layout, offer.object, offer.offer);
      if (offer.closed) {
        return offer.departs === null
          ? TEXT.house_why_closed.replace("{what}", what).replace("{time}", opens)
          : TEXT.house_why_departs.replace("{what}", what).replace("{time}", timeOfDay(offer.departs.date.second_of_day));
      }
      const told = TEXT.house_why_offer.replace("{what}", what).replace("{score}", score(offer, needs, true));
      return `${told}${offer.busy ? ` ${TEXT.house_why_busy}` : ""}${bedWords(offer)}${satedWords(offer, needs, comfort)}`;
    })
    .join(", ");
}

/** The needs an offer raises that weigh nothing, at or above their comfort, as the server names
 * them: " (Hunger zählt ab 700 und steht bei 750, das bringt nichts)", one per need. */
export function satedWords(offer: WhyOffer, needs: readonly number[], comfort: readonly number[]): string {
  return offer.sated
    .map((code) => {
      const index = NEED_TEXTS.findIndex(([need]) => need === code);
      const key = NEED_TEXTS[index]?.[1];
      return ` ${TEXT.house_why_sated
        .replace("{need}", key === undefined ? code : TEXT[key])
        .replace("{comfort}", String(comfort[index] ?? ""))
        .replace("{level}", String(needs[index] ?? ""))}`;
    })
    .join("");
}

/** Where a need that weighs nothing now counts from, its comfort below full, as the server gives
 * both (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler): " · zählt ab 700";
 * nothing for a need that weighs, or whose comfort is full. */
export function weightWords(weight: number | undefined, comfort: number | undefined): string {
  return weight === 0 && comfort !== undefined && comfort < 1000 ? ` · ${TEXT.house_weight_none.replace("{comfort}", String(comfort))}` : "";
}

/** The needs by their codes with their articles, joined by "und": "der Hunger und die Blase". */
function needNames(codes: readonly string[]): string {
  const named: { readonly [code: string]: string } = {
    hunger: TEXT.need_the_hunger,
    energy: TEXT.need_the_energy,
    bladder: TEXT.need_the_bladder,
    hygiene: TEXT.need_the_hygiene,
    fun: TEXT.need_the_fun,
  };
  return codes.map((code) => named[code] ?? code).join(TEXT.house_why_and);
}

/** Why a time-filler ends where it does, as the server says (ADR-0026, addendum of 2026-11-05
 * on the time-filler's length): ", dann ist die Zeit um", ", dann ist Schlafenszeit", ", dann
 * ist Aufbruch"; at the next choice what it raised, ", dann reicht die Blase"; nothing else. */
function fillerEnds(ends: string | null, then: readonly string[]): string {
  switch (ends) {
    case "time":
      return TEXT.house_why_filler_time;
    case "bedtime":
      return TEXT.house_why_filler_bedtime;
    case "departure":
      return TEXT.house_why_filler_departure;
    default:
      return then.length === 0
        ? ""
        : (then.length === 1 ? TEXT.house_why_filler_then : TEXT.house_why_filler_then_many).replace("{needs}", needNames(then));
  }
}

/** A time-filler as "Warum?" tells it, all from the server (ADR-0026, addenda of 2026-11-04 on
 * comfort and the time-filler and of 2026-11-05 on its length): "Zeitvertreib: liest am
 * Bücherregal, Laune +8 % (Fernseher +2 %, Sofa −4 %), bis 19:12, dann ist die Zeit um. 3 s Weg
 * ins Wohnzimmer." */
export function fillerWords(why: WhyAnswer, layout: HouseLayout): string {
  const filler = why.filler;
  const chosen = why.chosen;
  if (filler === null || chosen === null) {
    return "";
  }
  const others = filler.others.map((other) =>
    TEXT.house_why_filler_other.replace("{what}", objectName(layout, other.object)).replace("{mood}", signed(other.mood)),
  );
  const listed = others.length === 0 ? "" : TEXT.house_why_filler_others.replace("{list}", others.join(", "));
  const mood = filler.mood === null ? "" : `${TEXT.house_why_filler_mood.replace("{mood}", signed(filler.mood))}${listed}`;
  const until = filler.until === null ? "" : TEXT.house_why_filler_until.replace("{time}", timeOfDay(filler.until.date.second_of_day));
  const then = filler.until === null ? "" : fillerEnds(filler.ends, filler.then);
  const room = layout.objects[chosen.object]?.room ?? "";
  const walk = TEXT.house_why_filler_walk.replace("{secs}", String(filler.walk_secs)).replace("{where}", ROOM_WORDS[room]?.[1] ?? room);
  const what = placedOfferWords(layout, chosen.object, chosen.offer);
  return `${TEXT.house_why_filler.replace("{what}", what).replace("{mood}", mood).replace("{until}", until).replace("{then}", then)} ${walk}`;
}

/** Why an idle choice took no time-filler (ADR-0026, addendum of 2026-11-05 on the time-filler's
 * length), as the server says: "Zu kurz vor dem Aufbruch um 7:40."; nothing without one. */
export function shortWords(why: WhyAnswer): string {
  const short = why.too_short;
  if (short === null) {
    return "";
  }
  const texts: { readonly [ends: string]: string } = {
    departure: TEXT.house_why_short_departure,
    bedtime: TEXT.house_why_short_bedtime,
    time: TEXT.house_why_short_time,
  };
  const text = texts[short.ends] ?? TEXT.house_why_short_choice;
  return ` ${text.replace("{time}", timeOfDay(short.until.date.second_of_day))}`;
}

/** Why a bed of "Warum?" was not the person's (ADR-0026, addendum of 2026-11-04 after the
 * playtest), as the server says: a small child held a place of it, or in the night it was a bed
 * outside the person's tier, named by whom it is for. */
export function bedWords(offer: WhyOffer): string {
  if (offer.held) {
    return ` ${TEXT.house_why_held}`;
  }
  if (!offer.outside) {
    return "";
  }
  const whose = offer.bed_for === "couples" ? TEXT.house_why_couples : offer.bed_for === "children" ? TEXT.house_why_children : TEXT.house_why_other_bed;
  return ` ${whose}`;
}

/** Where an idle person went from a room one does not stay in (ADR-0026, addendum of
 * 2026-11-04 after the playtest), the room the server names. */
function goesWords(why: WhyAnswer, layout: HouseLayout): string {
  if (why.goes_to === null) {
    return "";
  }
  const id = layout.rooms[why.goes_to]?.id ?? "";
  return ` ${TEXT.house_why_goes.replace("{where}", ROOM_WORDS[id]?.[1] ?? id)}`;
}

/** What the night looks after before a bed, hunger and the bladder under their comfort as the
 * server gives it (ADR-0026, addendum of 2026-11-05 on the time-filler's length): "Hunger oder
 * Blase unter 700", or each with its own where they differ. */
export function beforeBed(comfort: readonly number[]): string {
  const [hunger, , bladder] = comfort;
  return hunger === bladder
    ? TEXT.house_why_night_comfort.replace("{comfort}", String(hunger ?? ""))
    : TEXT.house_why_night_comforts.replace("{hunger}", String(hunger ?? "")).replace("{bladder}", String(bladder ?? ""));
}

/** The sentence of "Warum?" on a person's reserve at the clock, by the server's band (ADR-0026,
 * addendum of 2026-11-06 on hunger at 0): "Vorrat noch 730 von 1000.", weak with what that does,
 * wasted with energy falling faster; null for the band `none` or without a reserve. */
export function reserveWords(reserve: HouseReserve | null): string | null {
  if (reserve === null) {
    return null;
  }
  const filled = (text: string) => text.replace("{value}", String(reserve.value)).replace("{capacity}", String(reserve.capacity));
  switch (reserve.band) {
    case "draws":
      return filled(TEXT.house_why_reserve_draws);
    case "weak":
      return filled(TEXT.house_why_reserve_weak);
    case "wasted":
      return filled(TEXT.house_why_reserve_wasted);
    default:
      return null;
  }
}

/** "Warum?" for a choice as text, all from the server's answer (ADR-0026 section 6 and its
 * addenda of 2026-11-04): in the night that the bed comes first; what was chosen or commanded
 * with its worth, else what was waited for, else that nothing brought enough, and then the
 * time-filler taken instead; what came behind, each offer once; and for an idle choice that is
 * still the person's state, `now`, the same comparison at the clock (the playtest of
 * 2026-11-04); for the person's latest choice its reserve at the clock (`reserveWords`). */
export function whyText(why: WhyAnswer, layout: HouseLayout): { readonly told: string; readonly now: string | null } {
  const chosen = why.chosen;
  const opens = why.night === null ? "" : timeOfDay(why.night.from.date.second_of_day);
  const waited = why.waits === null ? undefined : why.offers.find((offer) => offer.object === why.waits);
  const first = chosen ?? waited ?? null;
  const filler = why.filler !== null && chosen !== null;
  const behind = listed(
    layout,
    why.offers.filter((offer) => first === null || offer.object !== first.object || offer.offer !== first.offer),
    why.needs,
    why.comfort,
    opens,
    first === null || filler ? 4 : 3,
  );
  const ends = why.night?.until ?? null;
  const night =
    why.night !== null && why.night.now && !why.commanded
      ? `${(ends === null ? TEXT.house_why_night_last : TEXT.house_why_night.replace("{time}", timeOfDay(ends.date.second_of_day))).replace("{comfort}", beforeBed(why.comfort))} `
      : "";
  const said = (text: string, offer: WhyChoice | WhyOffer) =>
    text.replace("{what}", placedOfferWords(layout, offer.object, offer.offer)).replace("{score}", score(offer, why.needs, false));
  const none = TEXT.house_why_none.replace("{enough}", pointsText(why.enough));
  // A command to an offer today's departure closes has no points, its departure instead
  // (ADR-0026, addendum of 2026-11-05 on the time-filler's length).
  const departs = chosen?.departs ?? null;
  const commanded =
    chosen !== null && departs !== null && chosen.points === null
      ? TEXT.house_why_commanded_departs
          .replace("{what}", placedOfferWords(layout, chosen.object, chosen.offer))
          .replace("{time}", timeOfDay(departs.date.second_of_day))
      : null;
  const head = filler
    ? `${none} ${fillerWords(why, layout)}`
    : chosen !== null
      ? why.commanded
        ? (commanded ?? said(TEXT.house_why_commanded, chosen))
        : said(TEXT.house_why_chosen, chosen)
      : waited !== undefined
        ? said(TEXT.house_why_waits, waited)
        : `${none}${shortWords(why)}${goesWords(why, layout)}`;
  // The person's reserve at the clock, for its latest choice, one sentence after the choice.
  const reserve = reserveWords(why.reserve);
  const told = `${night}${head}${behind === "" ? "" : ` ${TEXT.house_why_behind.replace("{list}", behind)}`}${reserve === null ? "" : ` ${reserve}`}`;
  const now =
    why.now === null || why.now.offers.length === 0
      ? null
      : TEXT.house_why_now
          .replace("{time}", timeOfDay(why.now.at.date.second_of_day))
          .replace("{list}", listed(layout, why.now.offers, why.now.needs, why.comfort, opens, 4));
  return { told, now };
}
