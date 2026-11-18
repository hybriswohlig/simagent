/**
 * The household row on the house page: where each member stands, how wide the
 * canvas is, and where each first name sits under its figure. A height is the
 * body's centimetres in metres, so a 120 cm child stays shorter than a 180 cm
 * parent. The camera frames {@link FRAME_CM}, not whoever is tallest.
 */

/** CSS pixels of one place in the row. */
export const FAMILY_SLOT_PX = 120;
/** CSS pixels of the row's height. */
export const FAMILY_HEIGHT_PX = 300;
/** A house holds eight (ADR-0025). The row draws no more. */
export const FAMILY_LIMIT = 8;
/** The height the card and the row both frame, in centimetres. */
export const FRAME_CM = 200;
/** The framed height fills this share of the vertical view, as the card's camera does. */
export const FRAME_FILL = 0.86;
/** Metres between the centres of two neighbours, about 0.7 m. */
export const FAMILY_SPACING_M = 0.7;

export type FamilyPlace = {
  /** Metres from the middle of the row. Negative is to the viewer's left. */
  readonly x: number;
  /** The member's height in metres, centimetres divided by 100. */
  readonly heightM: number;
};

function limited(count: number): number {
  if (count < 0) return 0;
  if (count > FAMILY_LIMIT) return FAMILY_LIMIT;
  return count;
}

/** The width the card's camera keeps for one body of `heightM` metres. */
export function framedBodyM(heightM: number): number {
  return Math.max(heightM * 0.7, 0.7);
}

/** Canvas width in CSS pixels: one slot per member, and no more than eight. */
export function familyCanvasWidth(count: number): number {
  return limited(count) * FAMILY_SLOT_PX;
}

/**
 * Centres `spacingM` apart, in the order given, the row centred on zero.
 * Every height uses the same metres per centimetre.
 */
export function familyPlaces(heightsCm: readonly number[], spacingM: number): readonly FamilyPlace[] {
  const count = heightsCm.length;
  const mid = (count - 1) / 2;
  const places: FamilyPlace[] = [];
  for (let i = 0; i < count; i += 1) {
    places.push({ x: (i - mid) * spacingM, heightM: (heightsCm[i] ?? 0) / 100 });
  }
  return places;
}

/**
 * Metres the camera shows across the row. At least the centres
 * {@link FAMILY_SPACING_M} apart plus the width of one framed body. Wider when
 * framing {@link FRAME_CM} on this canvas already shows more, so a short row
 * is not pulled in to shrink that gap.
 */
export function familyViewWidthM(count: number): number {
  const n = limited(count);
  if (n === 0) return 0;
  const heightM = FRAME_CM / 100;
  const framed = (heightM / FRAME_FILL) * (familyCanvasWidth(n) / FAMILY_HEIGHT_PX);
  const row = (n - 1) * FAMILY_SPACING_M + framedBodyM(heightM);
  return Math.max(framed, row);
}

/** Percent across the canvas, 0 at the left and 100 at the right, where member `index` is named. */
export function familyNamePercent(index: number, count: number): number {
  const n = limited(count);
  const view = familyViewWidthM(n);
  if (n === 0 || view === 0) return 50;
  const x = (index - (n - 1) / 2) * FAMILY_SPACING_M;
  return (0.5 + x / view) * 100;
}

/**
 * The body values as the row draws them: every number to one decimal. Ages move a little
 * with every clock step; at one decimal (about five weeks of age, a tenth of a standard
 * deviation, a millimetre) the row is built again only when a figure would visibly change.
 */
export function visualKey(body: object): string {
  return JSON.stringify(body, (_key, value: unknown) => (typeof value === "number" ? Math.round(value * 10) / 10 : value));
}
