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
  assert.equal(parts(shapes, "pupil").length, 2);
  assert.equal(parts(shapes, "sclera").length, 2);
  assert.equal(parts(shapes, "brow").length, 2);
  assert.equal(parts(shapes, "rim").length, 2);
  for (const part of ["beard", "moustache", "freckle", "wrinkle", "spot", "laxity"]) {
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
  // Stage 7 is bald except for the rim; without density there is no hair at all.
  const bald = bust(params({ balding_stage: 7 }));
  assert.equal(parts(bald, "hair").length, 0);
  assert.equal(parts(bald, "rim").length, 2);
  const none = bust(params({ hair_density: 0 }));
  assert.equal(parts(none, "hair").length + parts(none, "rim").length, 0);
  const cap = (stage) => one(bust(params({ balding_stage: stage })), "hair").ry;
  assert.ok(cap(0) > cap(3) && cap(3) > cap(6));
  assert.equal(one(bust(params({ hair_density: 0.4 })), "hair").opacity, 0.4);
  // The beard is as dense as its density; coverage 0 is upper lip and chin, more coverage
  // reaches wider over the jaw.
  const beard = (set) => bust(params(set));
  assert.equal(one(beard({ beard_density: 0.5, beard_coverage: 0.8 }), "beard").opacity, 0.5);
  const lipAndChin = beard({ beard_density: 1, beard_coverage: 0 });
  one(lipAndChin, "moustache");
  const left = (shapes) => Number(one(shapes, "beard").d.split(" ")[1]);
  assert.ok(left(beard({ beard_density: 1, beard_coverage: 1 })) < left(lipAndChin));
  assert.equal(parts(beard({ beard_density: 0, beard_coverage: 1 }), "beard").length, 0);
  assert.equal(parts(bust(params({ freckle_density: 1 })), "freckle").length, 24);
  assert.equal(parts(bust(params({ freckle_density: 0.5 })), "freckle").length, 12);
  assert.equal(parts(bust(params({ face_wrinkles: 0.6 })), "wrinkle").length, 9);
});

// Grok's round 39, R2 (after the review of the sheet, G10d): freckles of radius 1.1, the skin
// mixed 0.7 toward their colour, opacity 0.9; an iris of radius 2.2 with a pupil of 0.8; three
// forehead lines, two crow's feet at each eye and a fold at each side of the nose, of width
// 0.7 and as strong as the wrinkles; a chin line and two cheek lines as strong as the
// laxity; six age spots of radius 2 as strong as their value; the breast a line in the
// skin's tone as strong as its development; shoulders up to 12 % broader and the neck up to
// 15 % thicker with the androgenic development.
test("the cues of the sheet's review are drawn as Grok set them", () => {
  const set = { skin_eumelanin: 0.3, skin_pheomelanin: 0.3 };
  const skin = gridColor(MELANIN_TO_SRGB.skin, 0.3, 0.3);
  const freckled = parts(bust(params({ ...set, freckle_density: 1 })), "freckle");
  assert.deepEqual(
    [freckled[0].rx, freckled[0].fill, freckled[0].opacity],
    [1.1, mixColor(skin, "#6b3a1f", 0.7), 0.9],
  );
  const eyes = bust(params(set));
  assert.deepEqual(parts(eyes, "iris").map((iris) => iris.rx), [2.2, 2.2]);
  const pupils = parts(eyes, "pupil");
  assert.deepEqual(pupils.map((pupil) => [pupil.rx, pupil.fill]), [[0.8, "#1a1410"], [0.8, "#1a1410"]]);
  // Each pupil is drawn over its iris.
  assert.ok(eyes.indexOf(pupils[0]) > eyes.indexOf(parts(eyes, "iris")[0]));
  const aged = bust(params({ ...set, face_wrinkles: 0.6, skin_laxity: 0.4, age_spots: 0.5 }));
  for (const wrinkle of parts(aged, "wrinkle")) {
    assert.deepEqual([wrinkle.width, wrinkle.opacity, wrinkle.stroke], [0.7, 0.6, mixColor(skin, "#000000", 0.35)]);
  }
  const lax = parts(aged, "laxity");
  assert.deepEqual(lax.map((line) => line.opacity), [0.4, 0.4, 0.4]);
  const spots = parts(aged, "spot");
  assert.deepEqual(spots.map((spot) => spot.rx), [2, 2, 2, 2, 2, 2]);
  assert.deepEqual([spots[0].opacity, spots[0].fill], [0.5, mixColor(skin, "#6b3a1f", 0.6)]);
  const breast = one(bust(params({ ...set, breast_development: 0.4 })), "breast");
  assert.deepEqual(
    [breast.stroke, breast.width, breast.opacity, breast.fill],
    [mixColor(skin, "#5a2a2a", 0.55), 1, 0.4, "none"],
  );
  assert.equal(parts(bust(params({ ...set, breast_development: 0 })), "breast").length, 0);
  const half = (androgenic) => 50 - Number(parts(bust(params({ androgenic_development: androgenic })), "torso")[0].d.split(" ")[1]);
  const neck = (androgenic) => one(bust(params({ androgenic_development: androgenic })), "neck").rx;
  assert.ok(Math.abs(half(1) / half(0) - 1.12) < 1e-12);
  assert.ok(Math.abs(neck(1) / neck(0) - 1.15) < 1e-12);
});

test("a value outside its range is held to it, a missing one takes the default", () => {
  assert.deepEqual(bust(params({ head_size_z: 9 })), bust(params({ head_size_z: 4 })));
  assert.deepEqual(bust(params({ skin_eumelanin: Number.NaN })), bust(params()));
  // A reader of an older contract gets no value for a newer key (ADR-0005 section 6).
  const missing = params({ head_size_z: 2, hair_curl: 0.7 });
  delete missing.skin_eumelanin;
  delete missing.beard_density;
  assert.deepEqual(bust(missing), bust(params({ head_size_z: 2, hair_curl: 0.7 })));
});

// Grok's round 39, R1 c: the red corner of the hair table (no eumelanin, all pheomelanin) is
// #ee5a28, and red hair of two MC1R copies reads as red.
test("the red corner of the hair table", () => {
  assert.equal(gridColor(MELANIN_TO_SRGB.hair, 0, 1), "#ee5a28");
});

// Astra's review of the cues: the places Grok set in R2, relative to the head.
test("the cues stand where Grok set them", () => {
  const shapes = bust(params({ face_wrinkles: 0.6, skin_laxity: 0.4, age_spots: 0.5, breast_development: 0.4 }));
  const head = one(shapes, "head");
  const { cx, cy, rx, ry } = head;
  const chin = cy + ry;
  const near = (a, b) => Math.abs(a - b) < 1e-9;
  const spots = parts(shapes, "spot");
  const places = [[-0.2, -0.46], [0.16, -0.38], [-0.46, 0.2], [0.48, 0.14], [-0.3, 0.36], [0.28, 0.42]];
  places.forEach(([x, y], i) => {
    assert.ok(near(spots[i].cx, cx + x * rx) && near(spots[i].cy, cy + y * ry), `spot ${i}`);
  });
  const wrinkles = parts(shapes, "wrinkle");
  [0.56, 0.46, 0.36].forEach((at, i) => {
    const line = wrinkles[i];
    assert.ok(near(line.y1, cy - ry * at) && near(line.y2, cy - ry * at), `forehead ${i}`);
    assert.ok(near(line.x1, cx - rx * 0.4) && near(line.x2, cx + rx * 0.4), `forehead ${i}`);
  });
  const folds = wrinkles.filter((line) => near(line.y1, cy + ry * 0.22));
  assert.equal(folds.length, 2);
  for (const fold of folds) {
    const side = Math.sign(fold.x1 - cx);
    assert.ok(near(fold.x1, cx + side * rx * 0.14) && near(fold.x2, cx + side * rx * 0.3) && near(fold.y2, cy + ry * 0.46));
  }
  const [chinLine, ...cheeks] = parts(shapes, "laxity");
  assert.ok(near(chinLine.y1, chin + 2) && near(chinLine.x1, cx - rx * 0.34) && near(chinLine.x2, cx + rx * 0.34));
  for (const cheek of cheeks) {
    const side = Math.sign(cheek.x1 - cx);
    assert.ok(near(cheek.x1, cx + side * rx * 0.58) && near(cheek.y1, cy + ry * 0.25));
    assert.ok(near(cheek.x2, cx + side * rx * 0.4) && near(cheek.y2, cy + ry * 0.62));
  }
  // The breast's depth is 6 · breast_development.
  const breast = one(shapes, "breast");
  assert.ok(breast.d.split(" ").includes(String(6 * 0.4)));
});
