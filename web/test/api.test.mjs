// The viewer takes an answer only of the kind and version it expects (ADR-0021 section 2).
import { test } from "node:test";
import assert from "node:assert/strict";

import { Refused, UnknownAnswer, accept, figureBytes, figureImage, figurePath, figureText, getJson, postJson, refusal } from "../dist/api.js";

const SERVER = { format: "simagent.view.runs", version: 1 };
const FILE = { format_version: 4 };

test("an answer of the expected kind and version is taken", () => {
  const body = { format: "simagent.view.runs", version: 1, runs: [] };
  assert.equal(accept(body, SERVER), body);
  const file = { format_version: 4, persons: [] };
  assert.equal(accept(file, FILE), file);
});

test("another kind, version or shape is refused", () => {
  const refused = [
    [{ format: "simagent.view.runs", version: 2 }, SERVER],
    [{ format: "simagent.view.run", version: 1 }, SERVER],
    [{ format: "simagent.view.runs", version: "1" }, SERVER],
    [{ format: "simagent.view.runs" }, SERVER],
    [{ version: 1 }, SERVER],
    [{ format_version: 1 }, SERVER],
    [{ format_version: 5 }, FILE],
    [{ format_version: "4" }, FILE],
    [{ format: "simagent.view.runs", version: 1 }, FILE],
    [null, SERVER],
    [[], FILE],
    ["{}", FILE],
    [4, FILE],
  ];
  for (const [body, expected] of refused) {
    assert.throws(() => accept(body, expected), UnknownAnswer, JSON.stringify(body));
  }
  assert.throws(() => accept({ format_version: 5 }, FILE), {
    message: 'expected {"format_version":4}, came {"format_version":5}',
  });
});

test("only routes of the server are fetched", async () => {
  for (const path of ["https://example.org/api/runs", "//example.org/api/runs", "/runs"]) {
    await assert.rejects(getJson(path, SERVER), { message: `not a route of the server: ${path}` });
  }
});

test("a refusal of the server is read with its code and the core's reason", () => {
  const error = { format: "simagent.view.error", version: 1, code: "input_rejected" };
  const read = refusal({ ...error, reason: "partners_of_wrong_sex" });
  assert.ok(read instanceof Refused);
  assert.deepEqual([read.code, read.reason], ["input_rejected", "partners_of_wrong_sex"]);
  assert.equal(refusal(error)?.reason, null);
  for (const body of [{ ...error, version: 2 }, { ...error, code: 1 }, { ...error, reason: 1 }, null, "x"]) {
    assert.equal(refusal(body), null, JSON.stringify(body));
  }
});

test("a write goes as JSON to a route of the server; a refusal is thrown", async () => {
  const ADVANCED = { format: "simagent.view.advanced", version: 1 };
  const sent = [];
  const answer = (body, status) => async (path, init) => {
    sent.push([path, init]);
    return new Response(body, { status });
  };
  globalThis.fetch = answer(JSON.stringify({ ...ADVANCED, closed_through: 5 }), 200);
  const json = await postJson("/api/runs/a/advance", { step: "year" }, ADVANCED);
  assert.equal(json["closed_through"], 5);
  assert.deepEqual(sent, [
    [
      "/api/runs/a/advance",
      {
        method: "POST",
        headers: { Accept: "application/json", "Content-Type": "application/json" },
        body: '{"step":"year"}',
      },
    ],
  ]);
  const locked = { format: "simagent.view.error", version: 1, code: "run_locked" };
  globalThis.fetch = answer(JSON.stringify(locked), 409);
  await assert.rejects(postJson("/api/runs/a/advance", {}, ADVANCED), (error) => {
    return error instanceof Refused && error.code === "run_locked" && error.reason === null;
  });
  globalThis.fetch = answer("no JSON", 500);
  await assert.rejects(postJson("/api/runs/a/advance", {}, ADVANCED), {
    message: "/api/runs/a/advance: 500",
  });
  globalThis.fetch = answer(JSON.stringify({ format: "simagent.view.advanced", version: 2 }), 200);
  await assert.rejects(postJson("/api/runs/a/advance", {}, ADVANCED), UnknownAnswer);
  const before = sent.length;
  await assert.rejects(postJson("https://example.org/api/runs", {}, ADVANCED), {
    message: "not a route of the server: https://example.org/api/runs",
  });
  assert.equal(sent.length, before, "nothing sent");
});

// Figure files are read here, from this page, and a path that leaves `./figure/` is not fetched.
test("figure files are read from this page", async () => {
  const calls = [];
  const previousFetch = globalThis.fetch;
  const previousBitmap = globalThis.createImageBitmap;
  globalThis.fetch = async (path) => {
    calls.push(String(path));
    if (String(path).endsWith(".png")) return new Response(new Uint8Array([1, 2, 3, 4]));
    if (String(path).endsWith(".json")) return new Response('{"ok":true}');
    return new Response(new Uint8Array([9, 8, 7]));
  };
  globalThis.createImageBitmap = async () => ({ kind: "bitmap" });
  try {
    for (const path of ["https://example.org/a", "//example.org/a", "./figure/../a", "/figure/a", "./other/a"]) {
      await assert.rejects(figureBytes(path), { message: `not a figure file: ${path}` });
    }
    assert.equal(calls.length, 0);
    const bytes = await figureBytes("./figure/body.glb");
    assert.deepEqual([...new Uint8Array(bytes)], [9, 8, 7]);
    assert.equal(await figureText("./figure/proxies/proxies.json"), '{"ok":true}');
    const image = await figureImage("./figure/proxies/brown_eye.png");
    assert.deepEqual(image, { kind: "bitmap" });
    assert.deepEqual(calls, [
      "./figure/body.glb",
      "./figure/proxies/proxies.json",
      "./figure/proxies/brown_eye.png",
    ]);
    globalThis.fetch = async () => new Response("", { status: 404 });
    await assert.rejects(figureText("./figure/proxies/proxies.json"), { message: "./figure/proxies/proxies.json: 404" });
  } finally {
    globalThis.fetch = previousFetch;
    globalThis.createImageBitmap = previousBitmap;
  }
});

test("a figure path stays under ./figure/", () => {
  assert.equal(figurePath("./figure/proxies/brown_eye.png"), "./figure/proxies/brown_eye.png");
  for (const bad of [
    "./figure/proxies/%2e%2e/%2e%2e/api/runs",
    "./figure/../api/runs",
    "./figure/proxies\\..\\x",
    "./figure/.hidden",
    "./figure/",
    "/figure/body.glb",
    "./api/runs",
  ]) {
    assert.throws(() => figurePath(bad), /not a figure file/, bad);
  }
});
