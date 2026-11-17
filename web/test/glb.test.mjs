import assert from "node:assert/strict";
import test from "node:test";

import { selfContained } from "../dist/render/glb.js";

/** A binary glTF with only a JSON chunk holding `json`. */
function glb(json) {
  const text = new TextEncoder().encode(JSON.stringify(json));
  const padded = Math.ceil(text.length / 4) * 4;
  const bytes = new ArrayBuffer(20 + padded);
  const view = new DataView(bytes);
  view.setUint32(0, 0x46546c67, true);
  view.setUint32(4, 2, true);
  view.setUint32(8, 20 + padded, true);
  view.setUint32(12, padded, true);
  view.setUint32(16, 0x4e4f534a, true);
  const body = new Uint8Array(bytes, 20, padded);
  body.fill(0x20);
  body.set(text);
  return bytes;
}

test("a body that carries its buffers and images itself is taken", () => {
  selfContained(glb({ asset: { version: "2.0" }, buffers: [{ byteLength: 4 }], images: [{ bufferView: 0 }] }));
});

test("a body that names an external buffer or image is refused", () => {
  assert.throws(() => selfContained(glb({ buffers: [{ uri: "https://example.org/x.bin", byteLength: 4 }] })), /external/);
  assert.throws(() => selfContained(glb({ images: [{ uri: "x.png" }] })), /external/);
});

test("bytes that are no binary glTF are refused", () => {
  assert.throws(() => selfContained(new ArrayBuffer(8)), /not a binary glTF/);
});
