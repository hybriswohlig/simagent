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
  | { kind: "path"; part: Part; d: string; fill: string; opacity: number }
  | { kind: "line"; part: Part; x1: number; y1: number; x2: number; y2: number; stroke: string; width: number; opacity: number };

export type Part =
  | "torso"
  | "neck"
  | "head"
  | "hair"
  | "sclera"
  | "iris"
  | "brow"
  | "freckle"
  | "wrinkle"
  | "beard"
  | "mouth";

/** The grey that hair turns to with `hair_grey_fraction` 1. */
const GREY = "#c4c4c4";
/** The colour freckles lean to from the skin. */
const FRECKLE = "#6b3a1f";
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

/** The shapes of the bust of `params`. */
export function bust(params: BodyParams): Shape[] {
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
  const chin = cy + ry;

  // The torso: shoulders by their width, muscle and fat, narrower before the body matures.
  const half =
    32 * (1 + 0.05 * v("shoulder_width_z") + 0.03 * v("muscle_z") + 0.03 * v("body_fat_z")) * (0.7 + 0.3 * body);
  const top = Math.min(chin + 8, 104);
  const chest = 6 * v("breast_development");
  const torso = `M ${50 - half} 120 Q ${50 - half} ${top} 50 ${top} Q ${50 + half} ${top} ${50 + half} 120 Z`;
  const bustLine = `M ${50 - half * 0.55} ${top + 12} q ${half * 0.275} ${chest} ${half * 0.55} 0 q ${half * 0.275} ${chest} ${half * 0.55} 0`;

  const shapes: Shape[] = [
    { kind: "path", part: "torso", d: torso, fill: skin, opacity: 1 },
    {
      kind: "ellipse", part: "neck", cx, cy: (chin + top) / 2, rx: rx * 0.42, ry: (top - chin) / 2 + 3,
      fill: skin, opacity: 1,
    },
  ];
  if (chest > 0) {
    shapes.push({ kind: "path", part: "torso", d: bustLine, fill: "none", opacity: 0.1 * chest });
  }
  shapes.push({ kind: "ellipse", part: "head", cx, cy, rx, ry, fill: skin, opacity: 1 });

  // The hair: a cap over the top of the head that ends above the brows, thinner with the
  // density, smaller and higher with each stage of balding, fuller with curls; none at the
  // last stage.
  const stage = v("balding_stage");
  const density = v("hair_density");
  if (density > 0 && stage < 7) {
    const cover = 1 - stage / 7;
    const full = 1.04 + 0.14 * v("hair_curl");
    shapes.push({
      kind: "ellipse", part: "hair", cx, cy: cy - ry * (0.78 + 0.22 * (1 - cover)),
      rx: rx * full * (0.55 + 0.45 * cover), ry: ry * 0.4 * cover, fill: hair, opacity: density,
    });
  }

  // The eyes, the brows, the mouth.
  const eyeY = cy - ry * 0.05;
  for (const side of [-1, 1]) {
    const x = cx + side * rx * 0.38;
    shapes.push({ kind: "ellipse", part: "sclera", cx: x, cy: eyeY, rx: 3.4, ry: 2.2, fill: "#f4f1ec", opacity: 1 });
    shapes.push({ kind: "ellipse", part: "iris", cx: x, cy: eyeY, rx: 1.6, ry: 1.6, fill: iris, opacity: 1 });
    shapes.push({
      kind: "line", part: "brow", x1: x - 3.6, y1: eyeY - 4.6, x2: x + 3.6, y2: eyeY - 5.2,
      stroke: hair, width: 1.3, opacity: v("eyebrow_density"),
    });
  }
  const mouthY = cy + ry * 0.48;
  shapes.push({
    kind: "line", part: "mouth", x1: cx - rx * 0.24, y1: mouthY, x2: cx + rx * 0.24, y2: mouthY,
    stroke: mixColor(skin, "#5a2a2a", 0.5), width: 1.1, opacity: 1,
  });

  // Freckles over nose and cheeks, as many of the places as the density says.
  const freckles = Math.round(v("freckle_density") * FRECKLE_PLACES.length);
  const freckle = mixColor(skin, FRECKLE, 0.45);
  for (const [fx, fy] of FRECKLE_PLACES.slice(0, freckles)) {
    shapes.push({ kind: "ellipse", part: "freckle", cx: cx + fx * rx, cy: cy + fy * ry, rx: 0.6, ry: 0.6, fill: freckle, opacity: 0.8 });
  }

  // Wrinkles across the forehead.
  const wrinkles = v("face_wrinkles");
  if (wrinkles > 0) {
    for (const row of [0, 1]) {
      const y = cy - ry * (0.5 - 0.1 * row);
      shapes.push({
        kind: "line", part: "wrinkle", x1: cx - rx * 0.4, y1: y, x2: cx + rx * 0.4, y2: y,
        stroke: mixColor(skin, "#000000", 0.35), width: 0.5, opacity: wrinkles,
      });
    }
  }

  // The beard: over chin and jaw, as dense as density times coverage.
  const beard = v("beard_density") * v("beard_coverage");
  if (beard > 0) {
    const jaw = `M ${cx - rx * 0.85} ${cy + ry * 0.2} Q ${cx} ${chin + ry * 0.35} ${cx + rx * 0.85} ${cy + ry * 0.2} Q ${cx} ${chin - ry * 0.05} ${cx - rx * 0.85} ${cy + ry * 0.2} Z`;
    shapes.push({ kind: "path", part: "beard", d: jaw, fill: hair, opacity: beard });
  }
  return shapes;
}
