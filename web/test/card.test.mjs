// The genetics card's lines of traits and height (Grok G2, G5, L1 to L6, M2): the percentile
// sentence by trait, sex and age, the parts with sign and two decimals, the parents' mean, the
// lines in cm. Every expected line is written by hand from those decisions.
import { test } from "node:test";
import assert from "node:assert/strict";

import { heightLines, parentsMeanSentence, partRows, signed, traitSentence } from "../dist/card.js";
import { bar } from "../dist/render/bar.js";

const row = (trait, up, n, adult) => ({ trait, z: 0, percentile: { up, n }, adult, parts: {}, parents_mean: null });

test("an adult's percentile sentence by trait and sex", () => {
  assert.equal(traitSentence(row("core:growth", true, 80, true), "male"), "größer als 80 von 100 Männern der Modellbevölkerung");
  assert.equal(traitSentence(row("core:growth", false, 64, true), "female"), "kleiner als 64 von 100 Frauen der Modellbevölkerung");
  assert.equal(traitSentence(row("core:cognition", true, 80, true), "male"), "höher als 80 von 100 Männern der Modellbevölkerung");
  assert.equal(traitSentence(row("core:cognition", false, 99, true), "female"), "niedriger als 99 von 100 Frauen der Modellbevölkerung");
  assert.equal(traitSentence(row("core:curl", true, 50, true), "female"), "lockiger als 50 von 100 Frauen der Modellbevölkerung");
  assert.equal(traitSentence(row("core:curl", false, 3, true), "male"), "glatter als 3 von 100 Männern der Modellbevölkerung");
});

test("a child's: the target height, and the same age for the others", () => {
  assert.equal(traitSentence(row("core:growth", true, 80, false), "male"), "wird größer als 80 von 100 Männern der Modellbevölkerung");
  assert.equal(traitSentence(row("core:growth", false, 70, false), "female"), "wird kleiner als 70 von 100 Frauen der Modellbevölkerung");
  assert.equal(traitSentence(row("core:cognition", true, 80, false), "male"), "höher als 80 von 100 Jungen gleichen Alters der Modellbevölkerung");
  assert.equal(traitSentence(row("core:curl", true, 80, false), "female"), "lockiger als 80 von 100 Mädchen gleichen Alters der Modellbevölkerung");
});

test("a trait the card does not know has no sentence", () => {
  assert.equal(traitSentence(row("core:pigment", true, 50, true), "male"), null);
});

test("the parents' mean without sex", () => {
  const mean = { ...row("core:cognition", true, 40, true), parents_mean: { up: true, n: 60 } };
  assert.equal(parentsMeanSentence(mean), "Eltern im Mittel: höher als 60 von 100 Personen der Modellbevölkerung");
  assert.equal(parentsMeanSentence(row("core:cognition", true, 40, true)), null);
});

test("a part with its sign, two decimals and a decimal comma", () => {
  assert.equal(signed(0.42), "+0,42");
  assert.equal(signed(-0.13), "−0,13");
  assert.equal(signed(0), "0,00");
  assert.equal(signed(-0.001), "0,00");
  assert.equal(signed(-0), "0,00");
  assert.equal(signed(1.006), "+1,01");
  assert.equal(signed(-2.5), "−2,50");
});

test("the four parts, or the inherited part with why", () => {
  const split = { kind: "split", parents: 0.42, passing: -0.13, childhood: 0, own: 0.25 };
  assert.deepEqual(partRows(split), {
    rows: [
      { label: "Elternanteil", value: 0.42, text: "+0,42" },
      { label: "Weitergabe", value: -0.13, text: "−0,13" },
      { label: "Gemeinsame Kindheit", value: 0, text: "0,00" },
      { label: "Eigenes Leben", value: 0.25, text: "+0,25" },
    ],
    note: null,
  });
  const none = { kind: "inherited", inherited: 1.5, childhood: -0.2, own: 0.1, parents_in_town: 0 };
  assert.deepEqual(partRows(none), {
    rows: [
      { label: "Erbanlage", value: 1.5, text: "+1,50" },
      { label: "Gemeinsame Kindheit", value: -0.2, text: "−0,20" },
      { label: "Eigenes Leben", value: 0.1, text: "+0,10" },
    ],
    note: "Eltern nicht in der Stadt",
  });
  assert.equal(partRows({ ...none, parents_in_town: 1 }).note, "Nur ein Elternteil in der Stadt");
  assert.equal(partRows({ kind: "other" }), null);
  assert.equal(partRows({ kind: "split", parents: "x" }), null);
  assert.equal(partRows(null), null);
  assert.equal(partRows("split"), null);
});

test("the lines in cm, each only when it is not 0,0", () => {
  assert.deepEqual(heightLines({ hunger_cm: 1.5, age_cm: 3, parents_target_cm: 174 }, 172.44), [
    "Größe jetzt: 172,4 cm",
    "Hungerjahre: −1,5 cm",
    "Alter: −3,0 cm",
    "Zielgröße der Eltern im Mittel: 174,0 cm",
  ]);
  assert.deepEqual(heightLines({ hunger_cm: 0.04, age_cm: 0, parents_target_cm: null }, 120), ["Größe jetzt: 120,0 cm"]);
});

test("a bar from the middle on the scale ±2, held at its ends", () => {
  assert.deepEqual(bar(1), { left: 50, width: 25 });
  assert.deepEqual(bar(-1), { left: 25, width: 25 });
  assert.deepEqual(bar(0), { left: 50, width: 0 });
  assert.deepEqual(bar(3), { left: 50, width: 50 });
  assert.deepEqual(bar(-5), { left: 0, width: 50 });
});
