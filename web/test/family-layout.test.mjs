// The household row: where each member stands, how wide the canvas is, and where
// each name sits. The expected numbers are written out. None is taken from the
// code under test.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  FAMILY_HEIGHT_PX,
  FAMILY_SPACING_M,
  FRAME_CM,
  FRAME_FILL,
  familyCanvasWidth,
  familyNamePercent,
  familyPlaces,
  familyViewWidthM,
  framedBodyM,
  visualKey,
} from "../dist/render/family-layout.js";

test("the canvas is one 120 px slot per member, at most eight", () => {
  assert.equal(FAMILY_HEIGHT_PX, 300);
  assert.equal(familyCanvasWidth(0), 0);
  assert.equal(familyCanvasWidth(1), 120);
  assert.equal(familyCanvasWidth(3), 360);
  assert.equal(familyCanvasWidth(8), 960);
  assert.equal(familyCanvasWidth(9), 960);
  assert.equal(familyCanvasWidth(-1), 0);
});

test("members stand in the order given, centred, at one scale", () => {
  assert.equal(FAMILY_SPACING_M, 0.7);
  assert.deepEqual(familyPlaces([180], 0.7), [{ x: 0, heightM: 1.8 }]);
  // Half a gap from the middle. 120 cm stays 1.2 m beside a parent of 1.8 m.
  assert.deepEqual(familyPlaces([120, 180], 0.7), [
    { x: -0.35, heightM: 1.2 },
    { x: 0.35, heightM: 1.8 },
  ]);
  assert.deepEqual(familyPlaces([100, 150, 200], 1), [
    { x: -1, heightM: 1 },
    { x: 0, heightM: 1.5 },
    { x: 1, heightM: 2 },
  ]);
  assert.deepEqual(familyPlaces([160, 160, 160, 80], 0.5), [
    { x: -0.75, heightM: 1.6 },
    { x: -0.25, heightM: 1.6 },
    { x: 0.25, heightM: 1.6 },
    { x: 0.75, heightM: 0.8 },
  ]);
  assert.deepEqual(familyPlaces([], 0.7), []);
});

test("one body keeps the width the card frames", () => {
  // 0.7 of the 2 m frame is 1.4 m. A shorter body stays at 0.7 m.
  assert.equal(FRAME_CM, 200);
  assert.equal(framedBodyM(2), 1.4);
  assert.equal(framedBodyM(0.5), 0.7);
});

test("the camera shows the whole row, and at least the framed height", () => {
  assert.equal(FRAME_FILL, 0.86);
  // One member is one body of 1.4 m. Framing 200 cm on a 120 by 300 slot shows
  // (2 / 0.86) * (120 / 300) = 0.930232558139535 m, so the body is wider.
  assert.equal(familyViewWidthM(1), 1.4);
  // Three centres span 1.4 m and one body adds 1.4 m: 2.8 m.
  assert.equal(familyViewWidthM(3), 2.8);
  // Eight centres span 4.9 m plus one body of 1.4 m is 6.3 m. Framing 200 cm on
  // 960 by 300 shows (2 / 0.86) * (960 / 300) = 7.44186046511628 m, which wins.
  assert.equal(familyViewWidthM(8), 7.44186046511628);
});

test("a name sits under its figure", () => {
  assert.equal(familyNamePercent(0, 1), 50);
  // The 2.8 m view puts the three centres at a quarter, the middle and three quarters.
  assert.equal(familyNamePercent(0, 3), 25);
  assert.equal(familyNamePercent(1, 3), 50);
  assert.equal(familyNamePercent(2, 3), 75);
});

test("an hour of age keeps the visual key, a tenth of a year changes it", () => {
  // One hour is 1 / (364 * 24) = 0.000114 of a year: 30.0 stays 30.0. 30.06 rounds to 30.1.
  const body = { body_form_age: 30.0, face_form_age: 30.0, height_cm: 171.24, face_axis_01: -0.53 };
  const hour = { ...body, body_form_age: 30.000114, face_form_age: 30.000114, height_cm: 171.2401 };
  const later = { ...body, body_form_age: 30.06, face_form_age: 30.06 };
  assert.equal(visualKey(hour), visualKey(body));
  assert.notEqual(visualKey(later), visualKey(body));
  assert.equal(visualKey(body), '{"body_form_age":30,"face_form_age":30,"height_cm":171.2,"face_axis_01":-0.5}');
});
