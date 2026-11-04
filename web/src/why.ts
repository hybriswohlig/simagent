import { placedOfferWords } from "./command.js";
import type { HouseLayout, WhyAnswer, WhyChoice, WhyOffer } from "./generated/export.js";
import { timeOfDay } from "./render/plan.js";
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
 * had no free place, an offer closed by day with the start of the night `opens`. */
function listed(layout: HouseLayout, offers: readonly WhyOffer[], needs: readonly number[], opens: string, most: number): string {
  return offers
    .slice(0, most)
    .map((offer) => {
      const what = placedOfferWords(layout, offer.object, offer.offer);
      if (offer.closed) {
        return TEXT.house_why_closed.replace("{what}", what).replace("{time}", opens);
      }
      const told = TEXT.house_why_offer.replace("{what}", what).replace("{score}", score(offer, needs, true));
      return `${told}${offer.busy ? ` ${TEXT.house_why_busy}` : ""}`;
    })
    .join(", ");
}

/** "Warum?" for a choice as text, all from the server's answer (ADR-0026 section 6 and its
 * addenda of 2026-11-04): in the night that the bed comes first; what was chosen or commanded
 * with its worth, else what was waited for, else that nothing brought enough; what came behind,
 * each offer once; and for an idle choice that is still the person's state, `now`, the same
 * comparison at the clock (the playtest of 2026-11-04). */
export function whyText(why: WhyAnswer, layout: HouseLayout): { readonly told: string; readonly now: string | null } {
  const chosen = why.chosen;
  const opens = why.night === null ? "" : timeOfDay(why.night.from.date.second_of_day);
  const waited = why.waits === null ? undefined : why.offers.find((offer) => offer.object === why.waits);
  const first = chosen ?? waited ?? null;
  const behind = listed(
    layout,
    why.offers.filter((offer) => first === null || offer.object !== first.object || offer.offer !== first.offer),
    why.needs,
    opens,
    first === null ? 4 : 3,
  );
  const ends = why.night?.until ?? null;
  const night =
    why.night !== null && why.night.now && !why.commanded
      ? `${ends === null ? TEXT.house_why_night_last : TEXT.house_why_night.replace("{time}", timeOfDay(ends.date.second_of_day))} `
      : "";
  const said = (text: string, offer: WhyChoice | WhyOffer) =>
    text.replace("{what}", placedOfferWords(layout, offer.object, offer.offer)).replace("{score}", score(offer, why.needs, false));
  const head =
    chosen !== null
      ? said(why.commanded ? TEXT.house_why_commanded : TEXT.house_why_chosen, chosen)
      : waited !== undefined
        ? said(TEXT.house_why_waits, waited)
        : TEXT.house_why_none.replace("{enough}", pointsText(why.enough));
  const told = `${night}${head}${behind === "" ? "" : ` ${TEXT.house_why_behind.replace("{list}", behind)}`}`;
  const now =
    why.now === null || why.now.offers.length === 0
      ? null
      : TEXT.house_why_now
          .replace("{time}", timeOfDay(why.now.at.date.second_of_day))
          .replace("{list}", listed(layout, why.now.offers, why.now.needs, opens, 4));
  return { told, now };
}
