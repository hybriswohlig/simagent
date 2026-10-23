// The log of a run is read page by page until `next` is null; windows of the run page are
// read one after another, each from where the last read that came through ended up to its
// own end, so a failed window is read again with the next (ADR-0021 section 2).
import { test } from "node:test";
import assert from "node:assert/strict";

import { enteredBy, readLog, reads, windows } from "../dist/log.js";

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
    "/api/runs/town/events?from=10&to=20",
    "/api/runs/town/events?from=20&to=30",
    "/api/runs/town/events?from=20&to=40",
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
  assert.deepEqual(asked, ["/api/runs/town/events?from=10&to=20", "/api/runs/town/events?from=20&to=30"]);
});

test("a person's log is read by subject", async () => {
  const asked = serve([{ events: [item(1)], next: null }]);
  await readLog("town", "subject=7");
  assert.deepEqual(asked, ["/api/runs/town/events?subject=7"]);
});

test("a window that ends where the last one ended asks for nothing", async () => {
  const asked = serve([{ events: [item(1)], next: null }]);
  const read = windows("town", 10);
  const [first, again] = await Promise.all([read(20), read(20)]);
  assert.deepEqual([first.length, again.length], [1, 0]);
  assert.deepEqual(asked, ["/api/runs/town/events?from=10&to=20"]);
});

// The addendum to ADR-0023 before S2: an input stamped `t` acts in the second after it
// (ADR-0004), so its entries are those of `t + 1`, read at once.
test("the entries of an input are read from the second it acts in", async () => {
  const asked = serve([{ events: [item(101)], next: null }]);
  const items = await enteredBy("town", 100);
  assert.deepEqual(
    items.map((i) => i.entry.id),
    [101],
  );
  assert.deepEqual(asked, ["/api/runs/town/events?from=101&to=101"]);
});

// The playtest before the fun test (F5): a run page opened again, after a person's page,
// showed an empty chronicle, as it read the log only from where the run stood. The run pages
// of a tab hold what they read per run; a page opened again shows it and reads on from
// where it ended.
test("a run page opened again recalls what it read and reads on from its end", () => {
  const held = reads();
  assert.deepEqual(held.recall("town", 50), { through: 50, items: [] });
  held.hold("town", { through: 60, items: [item(7)] });
  assert.deepEqual(held.recall("town", 60), { through: 60, items: [item(7)] });
  // The run went on meanwhile: the page reads the gap from 60.
  assert.deepEqual(held.recall("town", 90), { through: 60, items: [item(7)] });
  // Each run its own.
  assert.deepEqual(held.recall("gabel", 60), { through: 60, items: [] });
  // A read past where the run stands is of another run of that name: nothing of it.
  assert.deepEqual(held.recall("town", 59), { through: 59, items: [] });
});
