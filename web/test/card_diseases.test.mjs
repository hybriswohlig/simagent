// The genetics card's disease and skipped lines (Grok G1, G4, L7, L8, M1, M3, M4): the name,
// the copies, the status word and the line "übersprungen". Every expected line is written by
// hand from those decisions.
import { test } from "node:test";
import assert from "node:assert/strict";

import { copiesLines, diseaseName, skippedSentence, statusWord } from "../dist/carddiseases.js";

test("the six diseases by their names", () => {
  assert.equal(diseaseName("core:disease_ar"), "Zäher Atem");
  assert.equal(diseaseName("core:disease_ad_late"), "Später Verfall");
  assert.equal(diseaseName("core:disease_ad_partial"), "Ungleicher Ausbruch");
  assert.equal(diseaseName("core:disease_xr"), "Schwere Blutung");
  assert.equal(diseaseName("core:disease_mt"), "Mütterliches Augenlicht");
  assert.equal(diseaseName("core:disease_liability"), "Geteilte Schwere");
  assert.equal(diseaseName("core:disease_new"), "core:disease_new");
});

test("the copies by inheritance", () => {
  assert.deepEqual(copiesLines({ kind: "two", mother: true, father: false }), [
    "von der Mutter: Anlage",
    "vom Vater: keine Anlage",
  ]);
  assert.deepEqual(copiesLines({ kind: "mother_and_y", mother: false }), [
    "von der Mutter: keine Anlage",
    "vom Vater: Y, keine Kopie",
  ]);
  assert.deepEqual(copiesLines({ kind: "mother", mother: true }), ["von der Mutter: Anlage"]);
  assert.deepEqual(copiesLines({ kind: "liability", percentile: { up: true, n: 97 } }), [
    "Anlage höher als 97 von 100 der Modellbevölkerung",
  ]);
  assert.deepEqual(copiesLines({ kind: "liability", percentile: { up: false, n: 12 } }), [
    "Anlage niedriger als 12 von 100 der Modellbevölkerung",
  ]);
  assert.equal(copiesLines({ kind: "two", mother: "yes" }), null);
  assert.equal(copiesLines(null), null);
  assert.equal(copiesLines({ kind: "liability", percentile: null }), null);
});

test("the status words, by sex for a carrier", () => {
  assert.equal(statusWord({ kind: "not_affected" }, "male"), "nicht betroffen");
  assert.equal(statusWord({ kind: "carrier" }, "male"), "Träger, bleibt gesund");
  assert.equal(statusWord({ kind: "carrier" }, "female"), "Trägerin, bleibt gesund");
  assert.equal(statusWord({ kind: "penetrance_fallen" }, "female"), "Anlage, bricht nicht aus");
  assert.equal(statusWord({ kind: "not_yet" }, "female"), "Anlage, noch nicht ausgebrochen");
  assert.equal(statusWord({ kind: "resting" }, "male"), "Anlage, ruht");
  assert.equal(statusWord({ kind: "affected", year: 12, age_years: 45 }, "male"), "betroffen seit Jahr 12, mit 45 Jahren");
  assert.equal(statusWord({ kind: "affected", year: -30, age_years: 0 }, "female"), "betroffen seit der Geburt");
  assert.equal(statusWord({ kind: "affected", year: 5, age_years: 1 }, "female"), "betroffen seit Jahr 5, mit 1 Jahr");
  assert.equal(statusWord({ kind: "other" }, "female"), null);
  assert.equal(statusWord(null, "female"), null);
});

const names = new Map([
  [4, "Tanja Wolf"],
  [7, "Dieter Huber"],
]);

test("skipped: both copies through a parent", () => {
  const line = {
    locus: "core:mc1r",
    mother_copy: { kind: "grandmother", sim: 4 },
    father_copy: { kind: "grandfather", sim: 7 },
  };
  assert.equal(
    skippedSentence(line, names),
    "Rot übersprungen: die Kopie der Mutter stammt von ihrer Mutter Tanja Wolf, die Kopie des Vaters von seinem Vater Dieter Huber.",
  );
  const pool = { locus: "core:herc2", mother_copy: { kind: "pool" }, father_copy: { kind: "grandmother", sim: 4 } };
  assert.equal(
    skippedSentence(pool, names),
    "Helle Augen übersprungen: die Kopie der Mutter stammt aus dem Herkunftspool, die Kopie des Vaters von seiner Mutter Tanja Wolf.",
  );
  const back = { locus: "core:mc1r", mother_copy: { kind: "grandfather", sim: 7 }, father_copy: { kind: "pool" } };
  assert.equal(
    skippedSentence(back, names),
    "Rot übersprungen: die Kopie der Mutter stammt von ihrem Vater Dieter Huber, die Kopie des Vaters aus dem Herkunftspool.",
  );
});

test("a new copy is not skipped", () => {
  const one = { locus: "core:mc1r", mother_copy: { kind: "new" }, father_copy: { kind: "grandfather", sim: 7 } };
  assert.equal(
    skippedSentence(one, names),
    "Rot: die Kopie der Mutter ist bei ihr neu entstanden, die Kopie des Vaters stammt von seinem Vater Dieter Huber.",
  );
  const other = { locus: "core:herc2", mother_copy: { kind: "pool" }, father_copy: { kind: "new" } };
  assert.equal(
    skippedSentence(other, names),
    "Helle Augen: die Kopie der Mutter stammt aus dem Herkunftspool, die Kopie des Vaters ist bei ihm neu entstanden.",
  );
  const both = { locus: "core:mc1r", mother_copy: { kind: "new" }, father_copy: { kind: "new" } };
  assert.equal(skippedSentence(both, names), "Rot: beide Kopien sind neu entstanden.");
});

test("an unknown locus or shape has no sentence", () => {
  assert.equal(skippedSentence({ locus: "core:other", mother_copy: { kind: "new" }, father_copy: { kind: "new" } }, names), null);
  assert.equal(skippedSentence({ locus: "core:mc1r", mother_copy: { kind: "aunt" }, father_copy: { kind: "new" } }, names), null);
  assert.equal(skippedSentence({ locus: "core:mc1r", mother_copy: null, father_copy: { kind: "new" } }, names), null);
  assert.equal(skippedSentence({ locus: "core:mc1r", mother_copy: { kind: "new" }, father_copy: 3 }, names), null);
  assert.equal(skippedSentence(null, names), null);
});
