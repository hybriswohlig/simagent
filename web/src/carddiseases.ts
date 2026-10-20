import { object, type Sex } from "./card.js";
import { CODES, TEXT } from "./text.js";

type Open = { readonly [key: string]: unknown };

/** The six hereditary diseases by their code: Grok's names and one-liners (G1). The card does
 * not name their models. */
export const DISEASES: { readonly [code: string]: { readonly name: string; readonly line: string } } = {
  "core:disease_ar": {
    name: "Zäher Atem",
    line: "Von Geburt an sind die Atemwege verlegt, Menschen mit nur einer Kopie bleiben gesund, und die meisten Kranken werden trotzdem alt.",
  },
  "core:disease_ad_late": {
    name: "Später Verfall",
    line: "Eine einzige Kopie reicht, im mittleren Leben zerfällt die Bewegung, und das Leben wird kürzer.",
  },
  "core:disease_ad_partial": {
    name: "Ungleicher Ausbruch",
    line: "Die Anlage bricht bei Frauen meist und bei Männern selten aus, erst spät, und verkürzt dann das Leben.",
  },
  "core:disease_xr": {
    name: "Schwere Blutung",
    line: "Fast nur Söhne sind von Kindheit an betroffen, und Töchter mit einer Kopie bleiben gesund.",
  },
  "core:disease_mt": {
    name: "Mütterliches Augenlicht",
    line: "Das Augenlicht kann im jungen Erwachsenenalter nachlassen, nur über die Mutter und öfter bei Söhnen, und niemand stirbt daran.",
  },
  "core:disease_liability": {
    name: "Geteilte Schwere",
    line: "Viele kleine Anlagen wirken zusammen, die Krankheit beginnt im jungen Erwachsenenalter, und Geschwister trifft es viel häufiger als Fremde.",
  },
};

/** The name of a disease, or its code for one this view does not know. */
export function diseaseName(code: string): string {
  return DISEASES[code]?.name ?? code;
}

function carries(value: unknown): string | null {
  return typeof value === "boolean" ? (value ? TEXT.card_trait_copy : TEXT.card_no_copy) : null;
}

/** The copies of a disease (Grok L7): from the mother and the father; a man's X with the Y from
 * the father; the one mitochondrial copy; for Geteilte Schwere the percentile of its liability.
 * Null for an object of another shape. */
export function copiesLines(value: unknown): string[] | null {
  const copies = object(value);
  if (copies === null) {
    return null;
  }
  const mother = carries(copies.mother);
  switch (copies.kind) {
    case "two": {
      const father = carries(copies.father);
      return mother === null || father === null
        ? null
        : [`${TEXT.card_from_mother}: ${mother}`, `${TEXT.card_from_father}: ${father}`];
    }
    case "mother_and_y":
      return mother === null ? null : [`${TEXT.card_from_mother}: ${mother}`, `${TEXT.card_from_father}: ${TEXT.card_y}`];
    case "mother":
      return mother === null ? null : [`${TEXT.card_from_mother}: ${mother}`];
    case "liability": {
      const percentile = object(copies.percentile);
      if (typeof percentile?.up !== "boolean" || typeof percentile.n !== "number") {
        return null;
      }
      const word = percentile.up ? TEXT.card_higher : TEXT.card_lower;
      return [`${TEXT.card_liability} ${word} ${TEXT.card_than} ${percentile.n} ${TEXT.card_of_100} ${TEXT.card_model}`];
    }
    default:
      return null;
  }
}

/** "mit N Jahren", with one year "mit 1 Jahr" (Grok P1). */
function atAge(years: number): string {
  return `${TEXT.card_at} ${years} ${years === 1 ? TEXT.card_year_at : TEXT.card_years_at}`;
}

/** The chronicle's template of an onset entry in the calendar year `year` (Grok P1, P2, P3):
 * in the year of the onset "{0} erkrankt an X", else "{0} ist an X erkrankt, seit Jahr Y, mit
 * A Jahren", or "seit der Geburt" at 0 whole years; X is the name uninflected. Null for a
 * payload of another shape. */
export function onsetTemplate(payload: unknown, year: number): string | null {
  const onset = object(payload);
  const disease = onset?.disease;
  const since = onset?.since_year;
  const age = onset?.age_years;
  if (typeof disease !== "string" || typeof since !== "number" || typeof age !== "number") {
    return null;
  }
  const late = age === 0 ? TEXT.onset_from_birth : TEXT.onset_late;
  const template = since === year ? (CODES["payload core:disease_onset"] ?? "") : late;
  return template
    .replace("{disease}", diseaseName(disease))
    .replace("{year}", String(since))
    .replace("{age}", atAge(age));
}

/** The status word of a disease (Grok G4, L7, M1, M3, M4); null for another shape. */
export function statusWord(value: unknown, sex: Sex): string | null {
  const status = object(value);
  switch (status?.kind) {
    case "not_affected":
      return TEXT.card_not_affected;
    case "carrier":
      return sex === "female" ? TEXT.card_carrier_female : TEXT.card_carrier_male;
    case "penetrance_fallen":
      return TEXT.card_penetrance_fallen;
    case "not_yet":
      return TEXT.card_not_yet;
    case "resting":
      return TEXT.card_resting;
    case "affected":
      if (typeof status?.year !== "number" || typeof status.age_years !== "number") {
        return null;
      }
      return status.age_years === 0
        ? TEXT.card_affected_from_birth
        : `${TEXT.card_affected_since} ${status.year}, ${atAge(status.age_years)}`;
    default:
      return null;
  }
}

/** The name of a skipped locus (Grok L8, G9): red hair, light eyes. */
const SKIPPED_TRAITS: { readonly [locus: string]: string } = {
  "core:mc1r": TEXT.card_red,
  "core:herc2": TEXT.card_light_eyes,
};

/** Where a copy came from, after "stammt" (`parent` the mother or the father of the person). */
function source(copy: Open, parent: "mother" | "father", names: ReadonlyMap<number, string>): string | null {
  const name = (sim: unknown) => (typeof sim === "number" ? (names.get(sim) ?? `${TEXT.person} ${sim}`) : null);
  const of = (grandmother: boolean) =>
    parent === "mother"
      ? grandmother
        ? TEXT.card_from_her_mother
        : TEXT.card_from_her_father
      : grandmother
        ? TEXT.card_from_his_mother
        : TEXT.card_from_his_father;
  switch (copy.kind) {
    case "grandmother":
    case "grandfather": {
      const who = name(copy.sim);
      return who === null ? null : `${of(copy.kind === "grandmother")} ${who}`;
    }
    case "pool":
      return TEXT.card_from_pool;
    default:
      return null;
  }
}

/** The line "übersprungen" (Grok G7, L8), with the grandparents' names; a copy made new in a
 * parent is not skipped. Null for an unknown locus or shape. */
export function skippedSentence(value: unknown, names: ReadonlyMap<number, string>): string | null {
  const line = object(value);
  const trait = typeof line?.locus === "string" ? SKIPPED_TRAITS[line.locus] : undefined;
  const mother = object(line?.mother_copy);
  const father = object(line?.father_copy);
  if (trait === undefined || mother === null || father === null) {
    return null;
  }
  const motherNew = mother.kind === "new";
  const fatherNew = father.kind === "new";
  if (motherNew && fatherNew) {
    return `${trait}: ${TEXT.card_both_new}.`;
  }
  const fromMother = motherNew ? null : source(mother, "mother", names);
  const fromFather = fatherNew ? null : source(father, "father", names);
  if ((!motherNew && fromMother === null) || (!fatherNew && fromFather === null)) {
    return null;
  }
  if (!motherNew && !fatherNew) {
    return `${trait} ${TEXT.card_skipped}: ${TEXT.card_mothers_copy} ${TEXT.card_stems} ${fromMother}, ${TEXT.card_fathers_copy} ${fromFather}.`;
  }
  const mothers = motherNew ? TEXT.card_mothers_new : `${TEXT.card_mothers_copy} ${TEXT.card_stems} ${fromMother}`;
  const fathers = fatherNew ? TEXT.card_fathers_new : `${TEXT.card_fathers_copy} ${TEXT.card_stems} ${fromFather}`;
  return `${trait}: ${mothers}, ${fathers}.`;
}
