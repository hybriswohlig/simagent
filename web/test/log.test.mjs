// The log of a run is read page by page until `next` is null; windows of the run page are
// read one after another, each from where the last read that came through ended, so a
// failed window is read again with the next (ADR-0021 section 2).
import { test } from "node:test";
import assert from "node:assert/strict";

import { readLog, windows } from "../dist/log.js";

const EVENTS = { format: "simagent.view.events", version: 2 };
const DATE = { year: 0, day_of_year: 0, month: 0, day_of_month: 0, week: 0, weekday: 0, season: 0, second_of_day: 0 };
const item = (id) => ({ entry: { id, t: id, subjects: [], causes: [], payload: { "core:birth": {} } }, date: DATE });

/** A server stub: the answer for each asked path in turn, and the paths asked. */
function serve(answers) {
  const asked = [];
  globalThis.fetch = async (path) => {
    asked.push(path);
    const answer = answers.shift();
    if (answer === "fail") {
      throw new Error("offline");
    }
    return new Response(JSON.stringify({ ...EVENTS, ...answer }), { status: 200 });
  };
  return asked;
}

test("a read follows next until it is null", async () => {
  const asked = serve([
    { events: [item(1), item(2)], next: 2 },
    { events: [item(3)], next: 3 },
    { events: [], next: null },
  ]);
  const items = await readLog("town", "from=5");
  assert.deepEqual(
    items.map((i) => i.entry.id),
    [1, 2, 3],
  );
  assert.deepEqual(asked, [
    "/api/runs/town/events?from=5",
    "/api/runs/town/events?from=5&after=2",
    "/api/runs/town/events?from=5&after=3",
  ]);
});

test("windows go one after another, each from the end of the last that came through", async () => {
  const asked = serve([{ events: [item(1)], next: null }, "fail", { events: [item(2)], next: null }]);
  const read = windows("town", 10);
  assert.deepEqual(
    (await read(20)).map((i) => i.entry.id),
    [1],
  );
  await assert.rejects(read(30), { message: "offline" });
  assert.deepEqual(
    (await read(40)).map((i) => i.entry.id),
    [2],
  );
  assert.deepEqual(asked, [
    "/api/runs/town/events?from=10",
    "/api/runs/town/events?from=20",
    "/api/runs/town/events?from=20",
  ]);
});

test("a window asked while another is read waits for it", async () => {
  const asked = serve([
    { events: [item(1)], next: null },
    { events: [item(2)], next: null },
  ]);
  const read = windows("town", 10);
  const [first, second] = await Promise.all([read(20), read(30)]);
  assert.deepEqual([first.length, second.length], [1, 1]);
  assert.deepEqual(asked, ["/api/runs/town/events?from=10", "/api/runs/town/events?from=20"]);
});

test("a person's log is read by subject", async () => {
  const asked = serve([{ events: [item(1)], next: null }]);
  await readLog("town", "subject=7");
  assert.deepEqual(asked, ["/api/runs/town/events?subject=7"]);
});
