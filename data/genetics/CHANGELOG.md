# Genomkarte

Jede Änderung von `genome_map.json` erhöht `map_version` und steht hier (B9, ADR-0007).

## 2 (2026-10-08)

- Format 2 (ADR-0008): die Liste `traits` (leer) vor den Genorten; additive und
  Schwellen-Genorte tragen Effekt, `d_ref` und Ladungen in ihrer Wirkungsweise.
  `core:dummy` bleibt unverändert. Merkmale mit Referenzwerten kommen mit dem Vorlauf
  (B19), der sie misst.

## 1 (2026-10-08)

- Erste Karte: nur `core:dummy` (Autosom, Abschnitt 0, Platz 0, rezessiv, Krankheit,
  Mutation nur zum Alternativallel), der Genort des Durchstichs. Die Genorte der Faktoren
  und Krankheiten kommen in Schritt 7 und 9.
