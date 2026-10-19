// The portrait v0: the bust takes its colours from the melanin rule, grows and shrinks
// with the body values in the stated direction, and shows a part only when its value says
// so. The values are hand-set; no expected number is computed by the code under test.
import { test } from "node:test";
import assert from "node:assert/strict";

import { BODY_PARAM_KEYS, BODY_PARAMS, MELANIN_TO_SRGB } from "../dist/generated/body.js";
import { gridColor, mixColor, rampColor } from "../dist/render/color.js";
import { bust } from "../dist/render/portrait.js";

function params(set = {}) {
  return Object.fromEntries(BODY_PARAM_KEYS.map((key) => [key, set[key] ?? BODY_PARAMS[key].default]));
}

const parts = (shapes, part) => shapes.filter((shape) => shape.part === part);
const one = (shapes, part) => {
  const found = parts(shapes, part);
  assert.equal(found.length, 1, part);
  return found[0];
};

test("the colours come from the melanin rule", () => {
  const set = { skin_eumelanin: 0.6, skin_pheomelanin: 0.3, hair_eumelanin: 0.2, hair_pheomelanin: 0.8, iris_melanin: 0.7 };
  const shapes = bust(params(set));
  assert.equal(one(shapes, "head").fill, gridColor(MELANIN_TO_SRGB.skin, 0.6, 0.3));
  assert.equal(one(shapes, "hair").fill, gridColor(MELANIN_TO_SRGB.hair, 0.2, 0.8));
  for (const iris of parts(shapes, "iris")) {
    assert.equal(iris.fill, rampColor(MELANIN_TO_SRGB.iris, 0.7));
  }
  const grey = bust(params({ ...set, hair_grey_fraction: 1 }));
  assert.equal(one(grey, "hair").fill, "#c4c4c4");
  assert.equal(mixColor("#102030", "#c4c4c4", 0), "#102030");
});

test("the defaults show head, hair, eyes and brows, no beard, freckles or wrinkles", () => {
  const shapes = bust(params());
  for (const part of ["head", "neck", "hair", "mouth"]) {
    one(shapes, part);
  }
  assert.equal(parts(shapes, "iris").length, 2);
  assert.equal(parts(shapes, "sclera").length, 2);
  assert.equal(parts(shapes, "brow").length, 2);
  for (const part of ["beard", "freckle", "wrinkle"]) {
    assert.equal(parts(shapes, part).length, 0, part);
  }
});

test("sizes follow their values", () => {
  const head = (set) => one(bust(params(set)), "head");
  assert.ok(head({ head_size_z: 2 }).ry > head({ head_size_z: -2 }).ry);
  // A child's head is larger against its shoulders than an adult's.
  const ratio = (age) => {
    const shapes = bust(params({ body_form_age: age, face_form_age: age }));
    const torso = parts(shapes, "torso")[0].d.split(" ");
    const half = 50 - Number(torso[1]);
    return one(shapes, "head").rx / half;
  };
  assert.ok(ratio(5) > ratio(30));
  const half = (set) => 50 - Number(parts(bust(params(set)), "torso")[0].d.split(" ")[1]);
  assert.ok(half({ shoulder_width_z: 2 }) > half({ shoulder_width_z: -2 }));
});

test("hair thins and goes with balding, beard and freckles come with their values", () => {
  assert.equal(parts(bust(params({ balding_stage: 7 })), "hair").length, 0);
  assert.equal(parts(bust(params({ hair_density: 0 })), "hair").length, 0);
  const cap = (stage) => one(bust(params({ balding_stage: stage })), "hair").ry;
  assert.ok(cap(0) > cap(3) && cap(3) > cap(6));
  assert.equal(one(bust(params({ hair_density: 0.4 })), "hair").opacity, 0.4);
  const beard = one(bust(params({ beard_density: 0.5, beard_coverage: 0.8 })), "beard");
  assert.equal(beard.opacity, 0.4);
  assert.equal(parts(bust(params({ beard_density: 1, beard_coverage: 0 })), "beard").length, 0);
  assert.equal(parts(bust(params({ freckle_density: 1 })), "freckle").length, 24);
  assert.equal(parts(bust(params({ freckle_density: 0.5 })), "freckle").length, 12);
  assert.equal(parts(bust(params({ face_wrinkles: 0.6 })), "wrinkle").length, 2);
});

test("a value outside its range is held to it, a missing one takes the default", () => {
  assert.deepEqual(bust(params({ head_size_z: 9 })), bust(params({ head_size_z: 4 })));
  assert.deepEqual(bust(params({ skin_eumelanin: Number.NaN })), bust(params()));
});
