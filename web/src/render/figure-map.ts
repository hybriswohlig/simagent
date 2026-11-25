/**
 * Display mapping from a person's body values onto the shape keys, colours, hair and
 * clothes of the standing figure. Every input number comes from the core. The weights are
 * the contract part of the style sample: no editor extras, and no constant face offsets,
 * so a z of 0 stays a weight of 0.
 */

import { MELANIN_TO_SRGB, type BodyParams } from "../generated/body.js";
import { gridColor, mixColor, rampColor } from "./color.js";
import type { Sex } from "./portrait.js";

const GREY = "#c4c4c4";

const HAIR_STYLES = ["hair_short", "hair_medium", "hair_long", "hair_tied", "hair_braid"] as const;

const DYES: readonly (readonly [string, string])[] = [
  ["#6d7c8a", "#2c333c"],
  ["#8d735c", "#3a342c"],
  ["#5e7268", "#2e3832"],
  ["#8a8070", "#24312e"],
  ["#7a624e", "#343c4c"],
  ["#6a6258", "#4a433c"],
];

/** Children wear clear colours: the top stands out, the trousers stay calm. */
const CHILD_DYES: readonly (readonly [string, string])[] = [
  ["#d8574a", "#3f5f8a"],
  ["#e9b23f", "#4a5d78"],
  ["#4f97d1", "#8a7a5c"],
  ["#5fae78", "#4d5563"],
  ["#c9699f", "#3f5f8a"],
  ["#ee8a4f", "#3d5a4a"],
];

const FEMALE_SUITS = ["cloth_f_tee", "cloth_skirt", "cloth_shirt"] as const;
const FEMALE_SHOES = ["shoes_sneaker", "shoes_black", "shoes_brown"] as const;
const MALE_SUITS = ["cloth_f_tee", "cloth_shirt", "cloth_jacket"] as const;
const MALE_SHOES = ["shoes_sneaker", "shoes_brown", "shoes_black"] as const;

export type Outfit = {
  suit: string;
  shoes: string;
  top: string;
  bottom: string;
  child: boolean;
};

export type Shares = { male: number; female: number };

function clamp(value: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, value));
}

/** FNV-1a over the sim id, the low 32 bits. The same id always picks the same hair and clothes. */
export function hashSim(id: string): number {
  let hash = 2166136261;
  for (let i = 0; i < id.length; i += 1) {
    hash ^= id.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

/**
 * Baby, child, young and old shares of `age`, the body-form age in years. Up to 1 it is
 * the baby shape, at 10 to 12 the child shape, at 25 the young one, from 70 the old one;
 * between them two neighbours blend.
 */
export function ageSlices(age: number): readonly [number, number, number, number] {
  if (age <= 1) return [1, 0, 0, 0];
  if (age <= 10) return [(10 - age) / 9, (age - 1) / 9, 0, 0];
  if (age <= 12) return [0, 1, 0, 0];
  if (age <= 25) return [0, (25 - age) / 13, (age - 12) / 13, 0];
  if (age <= 70) return [0, 0, (70 - age) / 45, (age - 25) / 45];
  return [0, 0, 0, 1];
}

/** Male and female shares of the androgenic and estrogenic development, each clamped to [0, 1]. */
export function branchShares(body: BodyParams): Shares {
  const androgen = clamp(body.androgenic_development, 0, 1);
  const estrogen = clamp(body.estrogenic_development, 0, 1);
  return {
    male: 0.5 + 0.5 * androgen - 0.5 * estrogen,
    female: 0.5 - 0.5 * androgen + 0.5 * estrogen,
  };
}

function zeroKeys(weights: Record<string, number>): void {
  for (const key of [
    "neck_length",
    "hand_size",
    "foot_size",
    "ear_size",
    "ear_projection",
    "eye_roundness",
    "forehead_height",
    "buttock_volume",
    "buttock_lift",
    "waist_width",
    "nose_bridge",
    "nose_tip",
    "brow_arch",
    "lip_ratio",
    "breast_volume",
    "breast_height",
  ]) {
    weights[key] = 0;
  }
}

/** Shape-key name to weight. A male keeps both breast weights at 0. Provisional keys stay 0. */
export function morphWeights(body: BodyParams, sex: Sex): Record<string, number> {
  const [baby, child, young, old] = ageSlices(body.body_form_age);
  const { male, female } = branchShares(body);
  const weights: Record<string, number> = {};
  const slices = [baby, child, young, old];
  const names = ["baby", "child", "young", "old"] as const;
  for (let index = 0; index < names.length; index += 1) {
    const name = names[index];
    const slice = slices[index];
    if (name === undefined || slice === undefined) continue;
    weights[`age_male_${name}`] = slice * male;
    weights[`age_female_${name}`] = slice * female;
  }
  const fat = clamp(body.body_fat_z / 2, -1, 1);
  const muscle = clamp(body.muscle_z / 2, -1, 1);
  const dist = clamp(body.fat_distribution_z / 2, -1, 1);
  const heavy = Math.max(fat, 0);
  const thin = Math.max(-fat, 0);
  const bulk = Math.max(muscle, 0);
  const soft = Math.max(-muscle, 0);
  const belly = Math.max(dist, 0);
  const hips = Math.max(-dist, 0);
  for (const branch of ["male", "female"] as const) {
    const share = branch === "male" ? male : female;
    weights[`fat_${branch}_max`] = share * heavy * 1.5;
    weights[`fat_${branch}_min`] = share * thin * 1.5;
    weights[`muscle_${branch}_max`] = share * bulk * 0.45;
    weights[`muscle_${branch}_min`] = share * soft * 0.45;
  }
  const quarter = (value: number) => value / 4;
  weights["leg_upper"] = quarter(body.leg_ratio_z);
  weights["leg_lower"] = quarter(body.leg_ratio_z);
  weights["head_size"] = quarter(body.head_size_z);
  weights["shoulder_width"] = quarter(body.shoulder_width_z);
  weights["chest_depth"] = quarter(body.chest_depth_z);
  weights["pelvis_width"] = quarter(body.hip_width_z);
  weights["fat_central"] = heavy * (1.6 + 0.8 * belly) - thin * (1.15 + 0.4 * belly);
  weights["fat_belly"] = heavy * belly * 0.25;
  weights["fat_peripheral"] = heavy * (1.15 + 0.85 * hips) - thin * (0.85 + 0.45 * hips);
  weights["fat_upperarm"] = heavy * (2.1 + 0.6 * hips) - thin * (1.5 + 0.4 * hips);
  weights["muscle_arm"] = bulk * 1.8 - soft * 1.1;
  weights["muscle_shoulder"] = bulk * 0.55 - soft * 0.35;
  weights["muscle_chest"] = bulk * 1.6 - soft * 0.5;
  const breast = clamp(body.breast_development, 0, 1);
  const youngFemale = weights["age_female_young"] ?? 0;
  const oldFemale = weights["age_female_old"] ?? 0;
  const cup = clamp(breast / 0.5, 0, 1);
  weights["breast_min"] = sex === "male" ? 0 : youngFemale + oldFemale;
  weights["breast_max"] = sex === "male" ? 0 : cup * 0.34 + Math.max(0, breast - 0.5) * 1.32;
  weights["face_age_decr"] = clamp((body.body_form_age - body.face_form_age) / 20, 0, 1);
  weights["face_age_incr"] = clamp((body.face_form_age - body.body_form_age) / 45, 0, 1);
  for (let axis = 1; axis <= 12; axis += 1) {
    const key = `face_axis_${String(axis).padStart(2, "0")}`;
    weights[key] = quarter(body[key as keyof BodyParams]);
  }
  zeroKeys(weights);
  return weights;
}

/** Skin colour `#rrggbb` from the contract melanin table. */
export function skinColor(body: BodyParams): string {
  return gridColor(MELANIN_TO_SRGB.skin, body.skin_eumelanin, body.skin_pheomelanin);
}

/** Hair colour, then mixed toward grey by `hair_grey_fraction`. */
export function hairColor(body: BodyParams): string {
  const own = gridColor(MELANIN_TO_SRGB.hair, body.hair_eumelanin, body.hair_pheomelanin);
  return mixColor(own, GREY, body.hair_grey_fraction);
}

/** Iris colour from the contract melanin ramp. */
export function irisColor(body: BodyParams): string {
  return rampColor(MELANIN_TO_SRGB.iris, body.iris_melanin);
}

/** The hair proxy the sim id always picks. Balding is a separate cut. */
export function hairStyle(simId: string): string {
  const index = hashSim(simId) % HAIR_STYLES.length;
  return HAIR_STYLES[index] ?? "hair_short";
}

/** A child under this body-form age wears the short cut. */
const SHORT_HAIR_UNTIL = 3;

/** How much of the hair's cover the baby shape takes away: a baby's hair is fine. */
const BABY_HAIR_THIN = 0.45;

/** The cut that is worn: a small child and a balding male are forced onto the short proxy. */
export function wornHair(body: BodyParams, sex: Sex, simId: string): string {
  if (body.body_form_age < SHORT_HAIR_UNTIL) return "hair_short";
  if (sex === "male" && body.balding_stage >= 1) return "hair_short";
  return hairStyle(simId);
}

/** The share of the scalp the hair covers: the density, thinned by the baby share of the shape. */
export function hairCover(body: BodyParams): number {
  const [baby] = ageSlices(body.body_form_age);
  return clamp(body.hair_density, 0, 1) * (1 - BABY_HAIR_THIN * baby);
}

function dyeAt(table: readonly (readonly [string, string])[], index: number): readonly [string, string] {
  return table[index] ?? table[0] ?? ["#000000", "#000000"];
}

/**
 * Suit, shoes and the two dye colours. `ageYears` is the body-form age; 13 and under wears
 * the child outfit. The default is an adult.
 */
export function clothes(sex: Sex, simId: string, ageYears = 30): Outfit {
  const hash = hashSim(simId);
  const child = ageYears <= 13;
  let suit: string;
  let shoes: string;
  if (child) {
    // A baby sits, and the legs of the shorts tear on a seated thigh.
    suit = hash % 2 === 1 || ageYears <= 1 ? "cloth_f_tee" : "cloth_shorts";
    shoes = "shoes_sneaker";
  } else if (sex === "female") {
    const slot = hash % 3;
    suit = FEMALE_SUITS[slot] ?? "cloth_f_tee";
    shoes = FEMALE_SHOES[slot] ?? "shoes_sneaker";
  } else {
    const slot = hash % 3;
    suit = MALE_SUITS[slot] ?? "cloth_f_tee";
    shoes = MALE_SHOES[slot] ?? "shoes_sneaker";
  }
  const table = child ? CHILD_DYES : DYES;
  const [top, bottom] = dyeAt(table, (hash >>> 4) % table.length);
  return { suit, shoes, top, bottom, child };
}
