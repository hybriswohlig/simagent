// The buttons of the run page for the shocks and the switch (ADR-0023 section 6 and its
// addendum before S2): what each sends in the command schema, how the switch is labelled by
// where it stands, and where it stands from the server. Texts by hand from Grok's wording.
import { test } from "node:test";
import assert from "node:assert/strict";

import { UnknownAnswer } from "../dist/api.js";
import { HUNGERS, diseasesOn, shockInput, switchInput, switchLabel } from "../dist/shocks.js";

test("a hunger button sends its template", () => {
  assert.deepEqual(
    HUNGERS.map(([shock, label]) => [JSON.stringify(shockInput(shock)), label]),
    [
      ['{"trigger_shock":{"shock":"core:hunger_one_year"}}', "Ein Hungerjahr"],
      ['{"trigger_shock":{"shock":"core:hunger_three_years"}}', "Drei Hungerjahre"],
    ],
  );
});

test("the switch button turns the switch the other way and says so", () => {
  assert.equal(
    JSON.stringify(switchInput(false)),
    '{"set_world_param":{"change":{"core:hereditary_diseases":true}}}',
  );
  assert.equal(
    JSON.stringify(switchInput(true)),
    '{"set_world_param":{"change":{"core:hereditary_diseases":false}}}',
  );
  assert.equal(switchLabel(false), "Erbkrankheiten anschalten");
  assert.equal(switchLabel(true), "Erbkrankheiten ausschalten");
});

test("where the switch stands comes from the run's world", async () => {
  const asked = [];
  const answers = [
    { format: "simagent.view.world", version: 1, hereditary_diseases: true },
    { format: "simagent.view.world", version: 1, hereditary_diseases: false },
    { format: "simagent.view.world", version: 2, hereditary_diseases: true },
  ];
  globalThis.fetch = async (path) => {
    asked.push(path);
    return new Response(JSON.stringify(answers.shift()), { status: 200 });
  };
  assert.equal(await diseasesOn("town"), true);
  assert.equal(await diseasesOn("town"), false);
  await assert.rejects(diseasesOn("town"), UnknownAnswer);
  assert.deepEqual(asked, ["/api/runs/town/world", "/api/runs/town/world", "/api/runs/town/world"]);
});
