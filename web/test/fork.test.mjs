// Forking from the run page (ADR-0021 section 2 and its addendum G2; texts Grok round 28):
// the body the form sends for each choice of the hereditary diseases, and the text of a
// refusal. Every expected value by hand from the addendum's body and Grok's wording.
import { test } from "node:test";
import assert from "node:assert/strict";

import { Refused, UnknownAnswer } from "../dist/api.js";
import { forkBody, forkFailureText } from "../dist/fork.js";

test("the body of a fork for each choice of the hereditary diseases", () => {
  assert.equal(JSON.stringify(forkBody("gabel", 29, "same")), '{"run":"gabel","from_year":29,"changes":[]}');
  assert.equal(
    JSON.stringify(forkBody("gabel", 5, "on")),
    '{"run":"gabel","from_year":5,"changes":[{"core:hereditary_diseases":true}]}',
  );
  assert.equal(
    JSON.stringify(forkBody("g.2", 1, "off")),
    '{"run":"g.2","from_year":1,"changes":[{"core:hereditary_diseases":false}]}',
  );
});

test("a refused fork says why", () => {
  assert.equal(
    forkFailureText(new Refused("fork_point_out_of_range", null)),
    "In diesem Jahr kann keine Gabel beginnen.",
  );
  assert.equal(
    forkFailureText(new Refused("override_rejected", "override_param_twice")),
    "Die Gabel nennt eine Weltregel zweimal.",
  );
  assert.equal(forkFailureText(new Refused("run_exists", null)), "Diesen Namen trägt schon ein Lauf.");
  assert.equal(
    forkFailureText(new Refused("run_locked", null)),
    "Ein anderes Programm schreibt diesen Lauf gerade.",
  );
  assert.equal(
    forkFailureText(new Refused("manifest_invalid", null)),
    "Der Server hat die Gabel abgelehnt: manifest_invalid",
  );
  assert.equal(
    forkFailureText(new Refused("override_rejected", "new_reason")),
    "Der Server hat die Gabel abgelehnt: new_reason",
  );
  assert.equal(forkFailureText(new UnknownAnswer("x")), "Der Server sendet Daten in einer Form, die diese Ansicht nicht kennt.");
  assert.equal(forkFailureText(new Error("offline")), "Der Server antwortet nicht.");
});
