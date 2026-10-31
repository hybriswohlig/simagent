import { LOOK_WORDS, TRAIT_WORDS } from "./text.js";

/** The labels of the card's origin lines, in the card's order (Grok's round 45, H2). */
const LABELS = {
  hair: "Haar",
  eyes: "Augen",
  skin: "Haut",
  height: "Größe",
  curls: "Locken",
  freckles: "Sommersprossen",
  face: "Gesicht",
  smart: "Verstand",
  strong: "Kraft",
  beauty: "Schönheit",
} as const;

/** Where a word comes from, as a tail of its sentence (R3, H2); two copies take none. */
const SOURCES: { readonly [source: string]: string } = {
  mother: ", eher von der Mutter",
  father: ", eher vom Vater",
  both: ", von beiden",
  both_faint: ", von beiden, bei den Eltern kaum zu sehen",
  away: ", Eltern nicht in der Stadt",
  copies: "",
};

/** The words of the face axes (H4): the drawn direction first, the other second. */
const FACE_WORDS: { readonly [axis: number]: readonly [string, string] } = {
  1: ["langes Gesicht", "kurzes Gesicht"],
  2: ["breiter Kiefer", "schmaler Kiefer"],
  3: ["langes Kinn", "kurzes Kinn"],
  4: ["breite Wangen", "schmale Wangen"],
  5: ["große Augen", "kleine Augen"],
  6: ["weit stehende Augen", "eng stehende Augen"],
  7: ["hohe Brauen", "tiefe Brauen"],
  8: ["lange Nase", "kurze Nase"],
  9: ["breite Nase", "schmale Nase"],
  10: ["volle Lippen", "dünne Lippen"],
  11: ["breiter Mund", "schmaler Mund"],
  12: ["hohe Stirn", "niedrige Stirn"],
};

/** The copy lines of MC1R and HERC2 (H5): two copies and one. */
const COPY_NAMES = {
  red_copies: ["Rotes Haar", "Anlage für rotes Haar"],
  blue_copies: ["Blaue Augen", "Anlage für helle Augen"],
} as const;

type Line = { readonly [key: string]: unknown };

const text = (value: unknown): string => (typeof value === "string" ? value : "");

function copiesText(copies: readonly string[]): string | null {
  const [first, second] = copies;
  if (copies.length === 1) {
    return first === "mother" ? "die Kopie von der Mutter" : first === "father" ? "die Kopie vom Vater" : "die Kopie neu entstanden";
  }
  if (copies.length !== 2) {
    return null;
  }
  if (first === "new" && second === "new") {
    return "beide neu entstanden";
  }
  const mother = first === "mother" ? "eine Kopie von der Mutter" : "eine neu entstanden";
  const father = second === "father" ? (first === "mother" ? "eine vom Vater" : "eine Kopie vom Vater") : "eine neu entstanden";
  return `${mother}, ${father}`;
}

/** The sentence of one origin line of the core, or `null` for a line this viewer does not
 * know. The words come from the core's codes; the page only puts them in their sentence. */
export function originSentence(line: Line): string | null {
  const kind = text(line["kind"]);
  const source = SOURCES[text(line["source"])];
  switch (kind) {
    case "hair":
    case "eyes":
    case "skin":
    case "height":
    case "curls":
    case "freckles": {
      const words = LOOK_WORDS[kind] as { readonly [code: string]: string };
      const word = words[text(line["word"])];
      return word === undefined || source === undefined ? null : `${LABELS[kind]}: ${word}${source}.`;
    }
    case "freckles_red": {
      const word = (LOOK_WORDS.freckles as { readonly [code: string]: string })[text(line["word"])];
      const red = line["copies"] === 2 ? "mit dem roten Haar" : "mit der Anlage für rotes Haar";
      return word === undefined ? null : `${LABELS.freckles}: ${word}, ${red}.`;
    }
    case "face": {
      const words = FACE_WORDS[Number(line["axis"])];
      const word = words === undefined ? undefined : line["up"] === true ? words[0] : words[1];
      return word === undefined || source === undefined ? null : `${LABELS.face}: ${word}${source}.`;
    }
    case "smart":
    case "strong":
    case "beauty": {
      const word = (TRAIT_WORDS[kind] as { readonly [step: number]: string })[Number(line["step"])];
      return word === undefined || source === undefined ? null : `${LABELS[kind]}: ${word}${source}.`;
    }
    case "red_copies":
    case "blue_copies": {
      const copies = Array.isArray(line["copies"]) ? line["copies"].map(text) : [];
      const tail = copiesText(copies);
      const name = COPY_NAMES[kind][copies.length === 2 ? 0 : 1];
      return tail === null ? null : `${name}: ${tail}.`;
    }
    default:
      return null;
  }
}
