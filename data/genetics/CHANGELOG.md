# Genomkarte

Jede Änderung von `genome_map.json` erhöht `map_version` und steht hier (B9, ADR-0007).

## 3 (2026-10-10)

- Die Erbkrankheiten aus Schritt 9 (Spezifikation 3.2, ADR-0017 Abschnitt 3):
  `core:dummy` heißt jetzt `core:disease_ar`, an derselben Stelle (Autosom, Abschnitt 0,
  Platz 0), so bleiben Genome bitgleich. Neu: `core:disease_ad_late` (Autosom, Abschnitt
  2, Platz 0, dominant), `core:disease_ad_partial` (Autosom, Abschnitt 4, Platz 0,
  dominant), `core:disease_xr` (X, Platz 0, rezessiv) und `core:disease_mt` (mt, Platz 0;
  eine Kopie, die Wirkungsweise dominant heißt: ein Allel genügt). Alle gekennzeichnet als
  Krankheit, Mutation nur zum Alternativallel (3.3). Die Krankheiten liegen auf
  verschiedenen Abschnitten, damit sie nicht gekoppelt vererbt werden. Die Krankheit mit
  Schwelle braucht ein Merkmal der Anfälligkeit; das kommt mit den Merkmalen des Vorlaufs.

## 2 (2026-10-08)

- Format 2 (ADR-0008): die Liste `traits` (leer) vor den Genorten; additive und
  Schwellen-Genorte tragen Effekt, `d_ref` und Ladungen in ihrer Wirkungsweise.
  `core:dummy` bleibt unverändert. Merkmale mit Referenzwerten kommen mit dem Vorlauf
  (B19), der sie misst.

## 1 (2026-10-08)

- Erste Karte: nur `core:dummy` (Autosom, Abschnitt 0, Platz 0, rezessiv, Krankheit,
  Mutation nur zum Alternativallel), der Genort des Durchstichs. Die Genorte der Faktoren
  und Krankheiten kommen in Schritt 7 und 9.
