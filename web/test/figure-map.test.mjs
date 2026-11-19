// The figure mapping: age slices, branch shares, shape-key weights and the colours, hair
// and clothes the card draws. The expected numbers are hand-set from the mapping formulas.
// No expected number is computed by the code under test.
//
// FNV-1a (offset 2166136261, prime 16777619, low 32 bits) of a sim id, worked by hand:
// "0" is 890022063, "1" is 873244444, "a" is 3826002220, "10" is 468396612.
import { test } from "node:test";
import assert from "node:assert/strict";

import { BODY_PARAM_KEYS, BODY_PARAMS, MELANIN_TO_SRGB } from "../dist/generated/body.js";
import {
  ageSlices,
  branchShares,
  clothes,
  hairColor,
  hairStyle,
  irisColor,
  morphWeights,
  skinColor,
  wornHair,
} from "../dist/render/figure-map.js";

function params(set = {}) {
  return Object.fromEntries(BODY_PARAM_KEYS.map((key) => [key, set[key] ?? BODY_PARAMS[key].default]));
}

const Z_DRIVEN = [
  "leg_upper",
  "leg_lower",
  "head_size",
  "shoulder_width",
  "chest_depth",
  "pelvis_width",
  "fat_male_max",
  "fat_male_min",
  "fat_female_max",
  "fat_female_min",
  "muscle_male_max",
  "muscle_male_min",
  "muscle_female_max",
  "muscle_female_min",
  "fat_central",
  "fat_belly",
  "fat_peripheral",
  "fat_upperarm",
  "muscle_arm",
  "muscle_shoulder",
  "muscle_chest",
  "face_axis_01",
  "face_axis_02",
  "face_axis_03",
  "face_axis_04",
  "face_axis_05",
  "face_axis_06",
  "face_axis_07",
  "face_axis_08",
  "face_axis_09",
  "face_axis_10",
  "face_axis_11",
  "face_axis_12",
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
  "breast_volume",
  "breast_height",
  "nose_bridge",
  "nose_tip",
  "brow_arch",
  "lip_ratio",
];

test("age slices at 0.5, 5.5, 11, 18.5, 47.5 and 80", () => {
  assert.deepEqual(ageSlices(0.5), [1, 0, 0, 0]);
  assert.deepEqual(ageSlices(5.5), [0.5, 0.5, 0, 0]);
  assert.deepEqual(ageSlices(11), [0, 1, 0, 0]);
  assert.deepEqual(ageSlices(18.5), [0, 0.5, 0.5, 0]);
  assert.deepEqual(ageSlices(47.5), [0, 0, 0.5, 0.5]);
  assert.deepEqual(ageSlices(80), [0, 0, 0, 1]);
});

test("branch shares are the identity at the three corners", () => {
  assert.deepEqual(branchShares(params({ androgenic_development: 1, estrogenic_development: 0 })), {
    male: 1,
    female: 0,
  });
  assert.deepEqual(branchShares(params({ androgenic_development: 0, estrogenic_development: 1 })), {
    male: 0,
    female: 1,
  });
  assert.deepEqual(branchShares(params({ androgenic_development: 0.5, estrogenic_development: 0.5 })), {
    male: 0.5,
    female: 0.5,
  });
  const young = morphWeights(
    params({ androgenic_development: 1, estrogenic_development: 0, body_form_age: 25, face_form_age: 25 }),
    "male",
  );
  assert.equal(young.age_male_young, 1);
  assert.equal(young.age_female_young, 0);
  assert.equal(young.age_male_child, 0);
  assert.equal(young.age_male_old, 0);
});

test("a male never gets breast weights, even when the female age weight is 1", () => {
  const body = params({
    androgenic_development: 0,
    estrogenic_development: 1,
    body_form_age: 25,
    face_form_age: 25,
    breast_development: 1,
  });
  const male = morphWeights(body, "male");
  assert.equal(male.age_female_young, 1);
  assert.equal(male.breast_min, 0);
  assert.equal(male.breast_max, 0);
  const female = morphWeights(body, "female");
  assert.equal(female.breast_min, 1);
  assert.equal(female.breast_max, 1);
  const average = morphWeights(
    params({
      androgenic_development: 0,
      estrogenic_development: 1,
      body_form_age: 25,
      face_form_age: 25,
      breast_development: 0.5,
    }),
    "female",
  );
  assert.equal(average.breast_min, 1);
  assert.equal(average.breast_max, 0.34);
});

test("a body at all z of 0 gives zero for every z-driven key", () => {
  for (const sex of ["female", "male"]) {
    const weights = morphWeights(params(), sex);
    for (const key of Z_DRIVEN) {
      assert.equal(weights[key], 0, `${sex} ${key}`);
    }
  }
});

test("fat and leg keys follow the stated gains", () => {
  const weights = morphWeights(
    params({
      androgenic_development: 1,
      estrogenic_development: 0,
      body_form_age: 25,
      face_form_age: 25,
      body_fat_z: 2,
      leg_ratio_z: 4,
    }),
    "male",
  );
  assert.equal(weights.fat_male_max, 1.5);
  assert.equal(weights.fat_central, 1.6);
  assert.equal(weights.fat_peripheral, 1.15);
  assert.equal(weights.fat_upperarm, 2.1);
  assert.equal(weights.fat_belly, 0);
  assert.equal(weights.leg_upper, 1);
  assert.equal(weights.leg_lower, 1);
});

test("a skin colour at a grid corner equals the table entry", () => {
  const corner = MELANIN_TO_SRGB.skin.srgb[0][0];
  assert.equal(corner, "#f7e6dc");
  assert.equal(skinColor(params({ skin_eumelanin: 0, skin_pheomelanin: 0 })), corner);
  assert.equal(skinColor(params({ skin_eumelanin: 1, skin_pheomelanin: 1 })), "#3a1f13");
  assert.equal(skinColor(params({ skin_eumelanin: 0, skin_pheomelanin: 1 })), "#f4d5b7");
});

test("hair and iris colours are the table ends, and grey hair is the grey", () => {
  assert.equal(hairColor(params({ hair_eumelanin: 0, hair_pheomelanin: 0, hair_grey_fraction: 0 })), "#e9dfc6");
  assert.equal(hairColor(params({ hair_eumelanin: 0, hair_pheomelanin: 0, hair_grey_fraction: 1 })), "#c4c4c4");
  assert.equal(irisColor(params({ iris_melanin: 0 })), "#8fb3d9");
  assert.equal(irisColor(params({ iris_melanin: 1 })), "#2e1a0e");
});

test("hair style and clothes follow the sim id", () => {
  assert.equal(hairStyle("0"), "hair_tied");
  assert.equal(hairStyle("1"), "hair_braid");
  assert.equal(hairStyle("a"), "hair_short");
  assert.equal(hairStyle("10"), "hair_long");
  const bald = params({ balding_stage: 3 });
  assert.equal(wornHair(bald, "male", "1"), "hair_short");
  assert.equal(wornHair(bald, "female", "1"), "hair_braid");
  assert.deepEqual(clothes("male", "1"), {
    suit: "cloth_shirt",
    shoes: "shoes_brown",
    top: "#8d735c",
    bottom: "#3a342c",
    child: false,
  });
  assert.deepEqual(clothes("female", "1"), {
    suit: "cloth_skirt",
    shoes: "shoes_black",
    top: "#8d735c",
    bottom: "#3a342c",
    child: false,
  });
  assert.deepEqual(clothes("male", "1", 10), {
    suit: "cloth_shorts",
    shoes: "shoes_sneaker",
    top: "#6e7f8c",
    bottom: "#35423e",
    child: true,
  });
});
