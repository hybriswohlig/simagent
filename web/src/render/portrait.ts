/**
 * The portrait v0 (ADR-0021 section 6): a simple bust drawn from the body values of the
 * contract until the style sample replaces it. This module turns the values into shapes in
 * a 100 by 120 box (SVG geometry, which only `web/src/render/` may compute); the page only
 * draws them. Every value comes from the core; the shapes are the one thing computed here.
 */

import { BODY_PARAMS, MELANIN_TO_SRGB, type BodyParamKey, type BodyParams } from "../generated/body.js";
import { gridColor, mixColor, rampColor } from "./color.js";

/** One shape of the bust, drawn in this order. */
export type Shape =
  | { kind: "ellipse"; part: Part; cx: number; cy: number; rx: number; ry: number; fill: string; opacity: number }
  | { kind: "path"; part: Part; d: string; fill: string; stroke: string; width: number; opacity: number }
  | { kind: "line"; part: Part; x1: number; y1: number; x2: number; y2: number; stroke: string; width: number; opacity: number };

export type Part =
  | "torso"
  | "breast"
  | "neck"
  | "head"
  | "hair"
  | "locks"
  | "rim"
  | "sclera"
  | "iris"
  | "pupil"
  | "brow"
  | "freckle"
  | "spot"
  | "wrinkle"
  | "laxity"
  | "beard"
  | "moustache"
  | "mouth"
  | "nose";

/** The sex a portrait is drawn for; it chooses the hair's length. */
export type Sex = "female" | "male";

/** The grey that hair turns to with `hair_grey_fraction` 1. */
const GREY = "#c4c4c4";
/** The colour freckles and age spots lean to from the skin. */
const FRECKLE = "#6b3a1f";
/** The pupil. */
const PUPIL = "#1a1410";
/** Places of age spots on forehead and cheeks, relative to the head (Grok's round 39, R2). */
const SPOT_PLACES: readonly (readonly [number, number])[] = [
  [-0.2, -0.46], [0.16, -0.38], [-0.46, 0.2], [0.48, 0.14], [-0.3, 0.36], [0.28, 0.42],
];
/** Places of freckles over nose and cheeks, relative to the head (x and y in -1 to 1). */
const FRECKLE_PLACES: readonly (readonly [number, number])[] = [
  [-0.42, 0.18], [0.42, 0.18], [-0.3, 0.24], [0.3, 0.24], [-0.52, 0.28], [0.52, 0.28],
  [-0.18, 0.12], [0.18, 0.12], [-0.36, 0.34], [0.36, 0.34], [-0.08, 0.2], [0.08, 0.2],
  [-0.6, 0.12], [0.6, 0.12], [-0.24, 0.38], [0.24, 0.38], [-0.46, 0.06], [0.46, 0.06],
  [-0.12, 0.3], [0.12, 0.3], [-0.56, 0.38], [0.56, 0.38], [-0.3, 0.06], [0.3, 0.06],
];

/** A value of the body values, held to its range in the contract. */
function value(params: BodyParams, key: BodyParamKey): number {
  const { min, max } = BODY_PARAMS[key];
  const found = params[key];
  return Number.isFinite(found) ? Math.min(max, Math.max(min, found)) : BODY_PARAMS[key].default;
}

/** How far from a child's form to an adult's, 0 to 1, by the form age in years. */
function maturity(age: number): number {
  return Math.min(1, Math.max(0, age / 18));
}

/** The shapes of the bust of `params`, a person of `sex`. */
export function bust(params: BodyParams, sex: Sex): Shape[] {
  const v = (key: BodyParamKey) => value(params, key);
  const skin = gridColor(MELANIN_TO_SRGB.skin, v("skin_eumelanin"), v("skin_pheomelanin"));
  const hairOwn = gridColor(MELANIN_TO_SRGB.hair, v("hair_eumelanin"), v("hair_pheomelanin"));
  const hair = mixColor(hairOwn, GREY, v("hair_grey_fraction"));
  const iris = rampColor(MELANIN_TO_SRGB.iris, v("iris_melanin"));
  const face = maturity(v("face_form_age"));
  const body = maturity(v("body_form_age"));

  // The head: larger against the body in a child, by the head size.
  const ry = 17 * (1 + 0.04 * v("head_size_z")) * (1.2 - 0.2 * face);
  const rx = ry * (0.78 + 0.04 * face);
  const cx = 50;
  const cy = 18 + ry;

  // The twelve face axes (Grok's round 43, R2; 0 is the form before them): 01 the length,
  // 02 the jaw, 03 the chin, 04 the cheeks, 05 the eyes, 06 their spacing, 07 the brows,
  // 08 the nose's length, 09 its width, 10 the lips, 11 the mouth's width, 12 the forehead.
  const f = (n: number) => v(`face_axis_${String(n).padStart(2, "0")}` as BodyParamKey);
  const lower = 0.06 * ry * f(12);
  const eyeY = cy - ry * 0.05 + lower;
  const browUp = 0.06 * ry * f(7);
  const browWidth = 1.3 * (1 + 0.15 * f(7));
  const mouthY = cy + ry * 0.48 + lower;
  const eye = 1 + 0.12 * f(5);
  const eyeX = Math.max(rx * (0.38 + 0.04 * f(6)), 3.4 * eye + 0.6);
  const mouthHalf = rx * 0.24 * (1 + 0.12 * f(11));
  const jaw = 1 + 0.08 * f(2);
  // The outline holds the features at every end of the axes: each end of a brow, each eye's
  // sides and top, each corner of the mouth lies inside the ellipse of its half (the lower
  // one as narrow as the jaw) scaled by 0.85; where it would not, the cheeks, the crown or
  // the chin give way.
  const held: [number, number][] = [];
  for (const side of [-1, 1]) {
    const x = side * eyeX;
    held.push([x - 3.6, eyeY - 4.6 - browUp - browWidth / 2], [x + 3.6, eyeY - 5.2 - browUp - browWidth / 2]);
    held.push([x - 3.4 * eye, eyeY], [x + 3.4 * eye, eyeY], [x, eyeY - 2.2 * eye]);
    held.push([side * mouthHalf, mouthY]);
  }
  const narrow = (y: number) => (y > cy ? Math.min(1, jaw) : 1);
  const rxCheek = Math.max(rx * (1 + 0.06 * f(4)), ...held.map(([x, y]) => Math.abs(x) / 0.8 / narrow(y)));
  const tall = (above: boolean) =>
    held
      .filter(([, y]) => (y <= cy) === above)
      .map(([x, y]) => Math.abs(y - cy) / Math.sqrt(0.85 - (Math.abs(x) / (rxCheek * narrow(y))) ** 2));
  const ryTop = Math.max(ry * (1 + 0.07 * f(1)), ...tall(true));
  const ryBottom = Math.max(ryTop + 0.08 * ry * f(3), ...tall(false));
  const chin = cy + ryBottom;
  const k = 0.5523;
  const outline =
    `M ${cx - rxCheek} ${cy} C ${cx - rxCheek} ${cy - k * ryTop} ${cx - k * rxCheek} ${cy - ryTop} ${cx} ${cy - ryTop} ` +
    `C ${cx + k * rxCheek} ${cy - ryTop} ${cx + rxCheek} ${cy - k * ryTop} ${cx + rxCheek} ${cy} ` +
    `C ${cx + rxCheek} ${cy + k * ryBottom} ${cx + k * rxCheek * jaw} ${chin} ${cx} ${chin} ` +
    `C ${cx - k * rxCheek * jaw} ${chin} ${cx - rxCheek} ${cy + k * ryBottom} ${cx - rxCheek} ${cy} Z`;

  // The torso: shoulders by their width, muscle and fat, narrower before the body matures,
  // broader with the androgenic development (Grok's round 39, R2).
  const androgenic = v("androgenic_development");
  const half =
    32 *
    (1 + 0.05 * v("shoulder_width_z") + 0.03 * v("muscle_z") + 0.03 * v("body_fat_z")) *
    (0.7 + 0.3 * body) *
    (1 + 0.12 * androgenic);
  const top = Math.min(chin + 8, 104);
  const chest = 6 * v("breast_development");
  const torso = `M ${50 - half} 120 Q ${50 - half} ${top} 50 ${top} Q ${50 + half} ${top} ${50 + half} 120 Z`;
  const bustLine = `M ${50 - half * 0.55} ${top + 12} q ${half * 0.275} ${chest} ${half * 0.55} 0 q ${half * 0.275} ${chest} ${half * 0.55} 0`;

  const shapes: Shape[] = [{ kind: "path", part: "torso", d: torso, fill: skin, stroke: "none", width: 0, opacity: 1 }];

  // Women and girls wear their hair long: locks behind the neck and the head, beyond the
  // cheeks and fuller with curls, a tenth wider where they end, from the crown to 16 below the
  // chin; a newborn has none, they grow to that length by six; gone at stage 7.
  const stage = v("balding_stage");
  const density = v("hair_density");
  const grow = Math.min(1, v("face_form_age") / 6);
  const full = 1.04 + 0.14 * v("hair_curl");
  if (sex === "female" && density > 0 && stage < 7 && grow > 0) {
    const w = rxCheek * (full + 0.08);
    const h = ryTop * 1.1;
    const end = cy + grow * (ryBottom + 16);
    const flare = w * (1 + 0.1 * grow);
    shapes.push({
      kind: "path", part: "locks", fill: hair, stroke: "none", width: 0, opacity: density,
      d:
        `M ${cx - flare} ${end} L ${cx - w} ${cy} C ${cx - w} ${cy - k * h} ${cx - k * w} ${cy - h} ${cx} ${cy - h} ` +
        `C ${cx + k * w} ${cy - h} ${cx + w} ${cy - k * h} ${cx + w} ${cy} L ${cx + flare} ${end} ` +
        `Q ${cx} ${end - 6} ${cx - flare} ${end} Z`,
    });
  }
  shapes.push({
    kind: "ellipse", part: "neck", cx, cy: (chin + top) / 2, rx: rx * 0.42 * (1 + 0.15 * androgenic),
    ry: (top - chin) / 2 + 3, fill: skin, opacity: 1,
  });
  // The breast as a line in the skin's tone, as strong as its development.
  if (chest > 0) {
    shapes.push({
      kind: "path", part: "breast", d: bustLine, fill: "none", stroke: mixColor(skin, "#5a2a2a", 0.55), width: 1,
      opacity: v("breast_development"),
    });
  }
  shapes.push({ kind: "path", part: "head", d: outline, fill: skin, stroke: "none", width: 0, opacity: 1 });

  // The hair: a rim at the sides, which stays at every stage of balding ("7 is bald except
  // for the rim of hair"), and a cap over the top that ends above the brows, thinner with
  // the density, smaller and higher with each stage, fuller with curls, gone at stage 7.
  if (density > 0) {
    for (const side of [-1, 1]) {
      shapes.push({
        kind: "ellipse", part: "rim", cx: cx + side * rx * 0.9, cy: cy - ry * 0.3, rx: rx * 0.16,
        ry: ry * 0.32, fill: hair, opacity: density,
      });
    }
  }
  if (density > 0 && stage < 7) {
    const cover = 1 - stage / 7;
    shapes.push({
      kind: "ellipse", part: "hair", cx, cy: cy - ry * (0.78 + 0.22 * (1 - cover)) - 0.04 * ry * f(12),
      rx: rx * full * (0.55 + 0.45 * cover), ry: ry * 0.4 * cover, fill: hair, opacity: density,
    });
  }

  // The eyes, the brows, the nose, the mouth; the eyes' inner edges stay 1.2 apart.
  for (const side of [-1, 1]) {
    const x = cx + side * eyeX;
    shapes.push({ kind: "ellipse", part: "sclera", cx: x, cy: eyeY, rx: 3.4 * eye, ry: 2.2 * eye, fill: "#f4f1ec", opacity: 1 });
    shapes.push({ kind: "ellipse", part: "iris", cx: x, cy: eyeY, rx: 2.2 * eye, ry: 2.2 * eye, fill: iris, opacity: 1 });
    shapes.push({ kind: "ellipse", part: "pupil", cx: x, cy: eyeY, rx: 0.8 * eye, ry: 0.8 * eye, fill: PUPIL, opacity: 1 });
    shapes.push({
      kind: "line", part: "brow", x1: x - 3.6, y1: eyeY - 4.6 - browUp, x2: x + 3.6, y2: eyeY - 5.2 - browUp,
      stroke: hair, width: browWidth, opacity: v("eyebrow_density"),
    });
  }
  const feature = mixColor(skin, "#5a2a2a", 0.5);
  const noseTop = eyeY + 1.5;
  const noseBase = noseTop + 0.22 * ry * (1 + 0.15 * f(8));
  const noseHalf = 0.12 * rx * (1 + 0.2 * f(9));
  shapes.push({
    kind: "path", part: "nose", fill: "none", stroke: mixColor(skin, "#5a2a2a", 0.35), width: 0.7, opacity: 1,
    d: `M ${cx} ${noseTop} L ${cx - noseHalf * 0.3} ${noseBase} M ${cx - noseHalf} ${noseBase} Q ${cx} ${noseBase + 1.2} ${cx + noseHalf} ${noseBase}`,
  });
  shapes.push({
    kind: "line", part: "mouth", x1: cx - mouthHalf, y1: mouthY, x2: cx + mouthHalf, y2: mouthY,
    stroke: feature, width: Math.max(0.3, 1.1 * (1 + 0.35 * f(10))), opacity: 1,
  });

  // Freckles over nose and cheeks, as many of the places as the density says.
  const freckles = Math.round(v("freckle_density") * FRECKLE_PLACES.length);
  const freckle = mixColor(skin, FRECKLE, 0.7);
  for (const [fx, fy] of FRECKLE_PLACES.slice(0, freckles)) {
    shapes.push({ kind: "ellipse", part: "freckle", cx: cx + fx * rx, cy: cy + fy * ry, rx: 1.1, ry: 1.1, fill: freckle, opacity: 0.9 });
  }

  // Age spots on forehead and cheeks, as strong as their value.
  const spots = v("age_spots");
  if (spots > 0) {
    const spot = mixColor(skin, FRECKLE, 0.6);
    for (const [sx, sy] of SPOT_PLACES) {
      shapes.push({ kind: "ellipse", part: "spot", cx: cx + sx * rx, cy: cy + sy * ry, rx: 2, ry: 2, fill: spot, opacity: spots });
    }
  }

  // The lines of age (Grok's round 39, R2): three across the forehead, two crow's feet at
  // each eye's outer corner, a fold from each side of the nose to the mouth, all as strong
  // as the wrinkles; a line under the chin and one down each cheek as strong as the laxity.
  const line = mixColor(skin, "#000000", 0.35);
  const stroke = (part: Part, opacity: number) => (x1: number, y1: number, x2: number, y2: number): Shape => ({
    kind: "line", part, x1, y1, x2, y2, stroke: line, width: 0.7, opacity,
  });
  const wrinkles = v("face_wrinkles");
  if (wrinkles > 0) {
    const wrinkle = stroke("wrinkle", wrinkles);
    for (const at of [0.56, 0.46, 0.36]) {
      shapes.push(wrinkle(cx - rx * 0.4, cy - ry * at, cx + rx * 0.4, cy - ry * at));
    }
    for (const side of [-1, 1]) {
      const corner = cx + side * (eyeX + 3.4 * eye + 0.2);
      shapes.push(wrinkle(corner, eyeY - 0.6, corner + side * 2.4, eyeY - 1.8));
      shapes.push(wrinkle(corner, eyeY + 0.6, corner + side * 2.4, eyeY + 1.8));
      shapes.push(wrinkle(cx + side * rx * 0.14, cy + ry * 0.22, cx + side * rx * 0.3, cy + ry * 0.46));
    }
  }
  const laxity = v("skin_laxity");
  if (laxity > 0) {
    const sag = stroke("laxity", laxity);
    shapes.push(sag(cx - rx * 0.34, chin + 2, cx + rx * 0.34, chin + 2));
    for (const side of [-1, 1]) {
      shapes.push(sag(cx + side * rx * 0.58, cy + ry * 0.25, cx + side * rx * 0.4, cy + ry * 0.62));
    }
  }

  // The beard, as dense as its density: upper lip and chin at coverage 0, reaching over
  // the jaw to the cheeks as the coverage grows ("1 includes cheeks and neck").
  const beard = v("beard_density");
  if (beard > 0) {
    const coverage = v("beard_coverage");
    const wide = rx * (0.3 + 0.55 * coverage);
    const top = cy + ry * (0.62 - 0.42 * coverage);
    const jaw = `M ${cx - wide} ${top} Q ${cx} ${chin + ry * 0.3} ${cx + wide} ${top} Q ${cx} ${chin - ry * 0.1} ${cx - wide} ${top} Z`;
    shapes.push({ kind: "path", part: "beard", d: jaw, fill: hair, stroke: "none", width: 0, opacity: beard });
    shapes.push({
      kind: "ellipse", part: "moustache", cx, cy: mouthY - 2, rx: rx * 0.3, ry: 1.2, fill: hair, opacity: beard,
    });
  }
  return shapes;
}
