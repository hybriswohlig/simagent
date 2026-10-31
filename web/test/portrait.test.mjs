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
  const head = (set) => extent(one(bust(params(set)), "head"));
  assert.ok(height(head({ head_size_z: 2 })) > height(head({ head_size_z: -2 })));
  // A child's head is larger against its shoulders than an adult's.
  const ratio = (age) => {
    const shapes = bust(params({ body_form_age: age, face_form_age: age }));
    const torso = parts(shapes, "torso")[0].d.split(" ");
    const half = 50 - Number(torso[1]);
    return width(extent(one(shapes, "head"))) / 2 / half;
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
  // The frame of the default adult head (ry 17, rx 17 · 0.82, centre (50, 35)); the cues stand
  // in it, whatever the face axes do.
  const [cx, cy, rx, ry] = [50, 35, 17 * 0.82, 17];
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

/** The extent of a shape: an ellipse by its radii, a path by the points of its `d` (for
 * comparisons only), a line by its ends. */
function extent(shape) {
  if (shape.kind === "ellipse") {
    return { left: shape.cx - shape.rx, right: shape.cx + shape.rx, top: shape.cy - shape.ry, bottom: shape.cy + shape.ry };
  }
  if (shape.kind === "line") {
    return { left: Math.min(shape.x1, shape.x2), right: Math.max(shape.x1, shape.x2), top: Math.min(shape.y1, shape.y2), bottom: Math.max(shape.y1, shape.y2) };
  }
  const numbers = shape.d.match(/-?\d+(\.\d+)?(e-?\d+)?/g).map(Number);
  const xs = numbers.filter((_, i) => i % 2 === 0);
  const ys = numbers.filter((_, i) => i % 2 === 1);
  return { left: Math.min(...xs), right: Math.max(...xs), top: Math.min(...ys), bottom: Math.max(...ys) };
}

const axis = (n, f) => bust(params({ [`face_axis_${String(n).padStart(2, "0")}`]: f }));
const width = (e) => e.right - e.left;
const height = (e) => e.bottom - e.top;

test("each of the twelve face axes moves its feature the way Grok set it (round 43, R2)", () => {
  const head = (n, f) => extent(one(axis(n, f), "head"));
  // 01 the length, 03 the chin downwards, 04 the cheeks wider.
  assert.ok(height(head(1, 2)) > height(head(1, -2)), "01 length");
  assert.ok(head(3, 2).bottom > head(3, -2).bottom, "03 chin");
  assert.ok(width(head(4, 2)) > width(head(4, -2)), "04 cheeks");
  // 02 the jaw: wider low in the face, the top of the head the same.
  const jaw = (f) => one(axis(2, f), "head").d;
  assert.notEqual(jaw(2), jaw(-2), "02 jaw");
  assert.equal(head(2, 2).top, head(2, -2).top, "02 jaw leaves the crown");
  // 05 the eyes larger, 06 farther apart.
  const sclera = (n, f) => parts(axis(n, f), "sclera");
  assert.ok(sclera(5, 2)[0].rx > sclera(5, -2)[0].rx, "05 eyes");
  const apart = (f) => Math.abs(sclera(6, f)[1].cx - sclera(6, f)[0].cx);
  assert.ok(apart(2) > apart(-2), "06 spacing");
  // 07 the brows higher and stronger.
  const brow = (f) => parts(axis(7, f), "brow")[0];
  assert.ok(brow(2).y1 < brow(-2).y1 && brow(2).width > brow(-2).width, "07 brows");
  // 08 the nose longer, 09 wider.
  const nose = (n, f) => extent(one(axis(n, f), "nose"));
  assert.ok(height(nose(8, 2)) > height(nose(8, -2)), "08 nose length");
  assert.ok(width(nose(9, 2)) > width(nose(9, -2)), "09 nose width");
  // 10 the lips fuller, 11 the mouth wider.
  const mouth = (n, f) => one(axis(n, f), "mouth");
  assert.ok(mouth(10, 2).width > mouth(10, -2).width, "10 lips");
  assert.ok(width(extent(mouth(11, 2))) > width(extent(mouth(11, -2))), "11 mouth");
  // 12 the forehead higher: the features lower, the hairline higher.
  assert.ok(parts(axis(12, 2), "sclera")[0].cy > parts(axis(12, -2), "sclera")[0].cy, "12 features");
  assert.ok(one(axis(12, 2), "hair").cy < one(axis(12, -2), "hair").cy, "12 hairline");
});

test("at zero on every axis the face keeps today's form", () => {
  // The default adult: ry 17, rx 17 · 0.82, centre (50, 35); eyes at ± 0.38 rx, 0.05 ry above
  // the centre; the mouth 0.48 ry below it, half as wide as 0.24 rx, 1.1 thick.
  const shapes = bust(params());
  const [rx, ry, cx, cy] = [17 * 0.82, 17, 50, 35];
  const near = (a, b) => Math.abs(a - b) < 1e-9;
  const head = extent(one(shapes, "head"));
  assert.ok(near(head.left, cx - rx) && near(head.right, cx + rx), "head width");
  assert.ok(near(head.top, cy - ry) && near(head.bottom, cy + ry), "head height");
  const eyes = parts(shapes, "sclera");
  assert.ok(near(eyes[0].cx, cx - rx * 0.38) && near(eyes[1].cx, cx + rx * 0.38), "eyes");
  assert.ok(near(eyes[0].cy, cy - ry * 0.05) && near(eyes[0].rx, 3.4), "eye height and size");
  const mouth = one(shapes, "mouth");
  assert.ok(near(mouth.y1, cy + ry * 0.48) && near(mouth.x1, cx - rx * 0.24) && near(mouth.width, 1.1), "mouth");
});

test("the eyes never touch: their inner edges stay 1.2 apart", () => {
  const shapes = bust(params({ face_axis_05: 4, face_axis_06: -4 }));
  const [left, right] = parts(shapes, "sclera");
  assert.ok(right.cx - right.rx - (left.cx + left.rx) >= 1.2 - 1e-9);
});

test("a wider jaw puts the chin's two control points wider apart (02)", () => {
  // The control points on the chin's row: the lowest y of the outline.
  const chinRow = (f) => {
    const d = one(axis(2, f), "head").d;
    const numbers = d.match(/-?\d+(\.\d+)?(e-?\d+)?/g).map(Number);
    const points = [];
    for (let i = 0; i + 1 < numbers.length; i += 2) {
      points.push([numbers[i], numbers[i + 1]]);
    }
    const low = Math.max(...points.map(([, y]) => y));
    const xs = points.filter(([, y]) => y === low).map(([x]) => x);
    return Math.max(...xs) - Math.min(...xs);
  };
  assert.ok(chinRow(2) > chinRow(0) && chinRow(0) > chinRow(-2));
});

/** The outline of a path of cubic segments as a polygon, 40 points per segment. */
function polygon(d) {
  const numbers = d.match(/-?\d+(\.\d+)?(e-?\d+)?/g).map(Number);
  let [x, y] = numbers;
  const points = [];
  for (let i = 2; i + 5 < numbers.length; i += 6) {
    const [x1, y1, x2, y2, x3, y3] = numbers.slice(i, i + 6);
    for (let n = 0; n < 40; n += 1) {
      const t = n / 40;
      const u = 1 - t;
      points.push([
        u * u * u * x + 3 * u * u * t * x1 + 3 * u * t * t * x2 + t * t * t * x3,
        u * u * u * y + 3 * u * u * t * y1 + 3 * u * t * t * y2 + t * t * t * y3,
      ]);
    }
    [x, y] = [x3, y3];
  }
  return points;
}

/** Whether `[px, py]` lies inside the polygon (even-odd rule). */
function inside(points, [px, py]) {
  let odd = false;
  for (let i = 0, j = points.length - 1; i < points.length; j = i, i += 1) {
    const [xi, yi] = points[i];
    const [xj, yj] = points[j];
    if (yi > py !== yj > py && px < ((xj - xi) * (py - yi)) / (yj - yi) + xi) {
      odd = !odd;
    }
  }
  return odd;
}

test("at the ends of every axis the head holds brows, eyes, nose and mouth", () => {
  const ends = [-4, 4];
  const axes = [1, 2, 3, 4, 5, 6, 7, 8, 11, 12];
  for (let pick = 0; pick < 1 << axes.length; pick += 1) {
    const set = Object.fromEntries(axes.map((n, i) => [`face_axis_${String(n).padStart(2, "0")}`, ends[(pick >> i) & 1]]));
    const shapes = bust(params(set));
    const outline = polygon(one(shapes, "head").d);
    const mouth = one(shapes, "mouth");
    const label = JSON.stringify(set);
    const held = [[mouth.x1, mouth.y1], [mouth.x2, mouth.y2]];
    for (const brow of parts(shapes, "brow")) {
      held.push([brow.x1, brow.y1 - brow.width / 2], [brow.x2, brow.y2 - brow.width / 2]);
    }
    for (const eye of parts(shapes, "sclera")) {
      held.push([eye.cx - eye.rx, eye.cy], [eye.cx + eye.rx, eye.cy], [eye.cx, eye.cy - eye.ry]);
    }
    for (const point of held) {
      assert.ok(inside(outline, point), `${point} outside: ${label}`);
    }
    assert.ok(extent(one(shapes, "nose")).bottom < mouth.y1, `nose above the mouth: ${label}`);
  }
});
