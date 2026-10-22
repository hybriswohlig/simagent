/** The page of the model's assumptions, Grok's text (O1, 2026-10-20) in his sections. */
export const ASSUMPTIONS: readonly { readonly heading: string; readonly paragraphs: readonly string[] }[] = [
  {
    heading: "Die Modellbevölkerung",
    paragraphs: [
      "Die Modellbevölkerung ist eine Stadt, die einmal vor dem Spiel gelaufen ist. Die Vergleichswerte daraus sind eingefroren, und sie gelten für jeden Lauf. Jedes Perzentil auf der Karte misst gegen Menschen desselben Geschlechts in dieser Stadt, nicht gegen die Menschen im laufenden Spiel.",
      "Weil der Vergleich feststeht, bleibt Drift sichtbar. In einer endlichen Stadt wandern die Anlagen, und eine spätere Generation kann im Mittel höher oder tiefer liegen. Eine Rangliste der Lebenden würde das jedes Jahr auf null setzen.",
      "Keine dieser Zahlen ist eine IQ-Skala. Das gilt auch für die Kognition.",
      "Unter 18 messen Kognition und Locken gegen gleichaltrige Jungen oder Mädchen der Modellbevölkerung.",
    ],
  },
  {
    heading: "Die vier Posten",
    paragraphs: [
      "Vier Posten addieren sich zum Wert hinter dem Perzentil.",
      "Der Elternanteil ist, was beide Eltern im Mittel weitergeben. Die Weitergabe ist, welche Kopien dieses Kind bekommen hat. Vollgeschwister teilen den Elternanteil und können sich in der Weitergabe unterscheiden. Die gemeinsame Kindheit teilen nur Vollgeschwister. Das eigene Leben gehört nur dieser Person.",
      "Stehen nicht beide Eltern in der Stadt, stehen Elternanteil und Weitergabe zusammen als „Erbanlage“.",
      "„Eltern im Mittel“ ist ein Vergleich und geht nicht in die Summe.",
    ],
  },
  {
    heading: "Größe",
    paragraphs: [
      "Oben steht die Größe jetzt, in Zentimetern. Das Perzentil gilt der Zielgröße, der angelegten Größe im Erwachsenenalter. Bei Kindern steht dort „wird“.",
      "Hungerjahre ziehen Zentimeter ab, die bleiben. Sie kommen aus knappen Jahren, am meisten in den ersten Lebensjahren. Ab dreißig zieht das Alter weitere Zentimeter ab. Beides fehlt im Perzentil.",
    ],
  },
  {
    heading: "Verwandtschaft",
    paragraphs: [
      "Neben dem Verwandtschaftswort steht ein Prozent, etwa „Cousine, 12,5 Prozent“. Die Zahl ist r = 2φ. φ misst die gemeinsame Abstammung, und r ist ihr Doppeltes in Prozent: der Anteil der Erbanlagen, den beide darüber erwartbar teilen.",
      "Ohne Schleife im Stammbaum sind das 50 Prozent bei Eltern und Kind und bei Vollgeschwistern, 25 Prozent bei Großeltern, Halbgeschwistern, Tante oder Onkel, 12,5 Prozent bei Cousins ersten Grades.",
      "Die Rechnung reicht bis zu den Urgroßeltern beider. Wer weiter zurückliegt, zählt nicht mit.",
      "Die mittlere Inzucht der Neugeborenen ist F = φ der Eltern und damit die Hälfte des Verwandtschaftsprozents, F = φ = r/2: Eltern, die als Cousins 12,5 Prozent teilen, geben ihrem Kind 6,25 Prozent Inzucht.",
    ],
  },
  {
    heading: "Erbkrankheiten",
    paragraphs: [
      "Die sechs Krankheiten haben erfundene Namen. Ihre Erbgänge sind echt: eine Kopie oder zwei, nur über die Mutter, oder viele kleine Anlagen zusammen. Die Karte nennt keine Vorbilder.",
      "Ein Lauf kann die Erbkrankheiten ausschalten. Dann erkrankt niemand, und die Krankheiten ändern kein Leben. Die Kopien werden weiter vererbt. Darüber steht: „Erbkrankheiten sind in diesem Lauf ausgeschaltet.“ Wo sonst eine Krankheit beginnen würde, steht „Anlage, ruht“.",
    ],
  },
  {
    heading: "Herkunft einer übersprungenen Kopie",
    paragraphs: [
      "Sehr selten nennt die Zeile bei übersprungenem Rot oder hellen Augen eine Großmutter oder einen Großvater, obwohl die Kopie erst bei der Weitergabe neu entstanden ist.",
    ],
  },
];
