# Genomkarte

Jede Änderung von `genome_map.json` erhöht `map_version` und steht hier (B9, ADR-0007).

## 6 (2026-11-25)

- Zwei Haarmerkmale (ADR-0034 Abschnitt 1): `core:hair_thickness` mit zwei eigenen
  additiven autosomalen Genorten `core:hair_thickness_01` (Segment 0, Platz 17) und
  `core:hair_thickness_02` (Segment 1, Platz 19), Effekt 1, Ladung +1, Klasse klein,
  Mutation in beide Richtungen; die Plätze sind die kleinsten freien der Karte 5 nach der
  Regel aus ADR-0020 Abschnitt 1, die alten Genorte behalten ihre Stellen.
  `core:hair_dryness` ohne eigene Genorte, `includes` `core:androgen` mit Gewicht −1.
  Kartenformat unverändert, `map_version` 6: 170 Genorte, 45 Merkmale, 24 freie Plätze.
- Referenzwerte: `core:hair_thickness` `d_ref` 1 je Genort, `sigma_ref` 1, `mean_ref` 0, die
  exakten Parameter der Hardy-Weinberg-Verteilung bei p = 0.5 (`Var(A) = 2 · 2p(1−p) = 1`),
  eine Ausnahme vom Messen. `core:hair_dryness` übernimmt `sigma_ref` von `core:androgen`
  und dessen negiertes `mean_ref`, so dass `G` bitgleich `−G` von `core:androgen` ist.
- Kalibrierung: `h2` 0.60 und `c2` 0 für `core:hair_thickness`, `h2` 0.15 und `c2` 0 für
  `core:hair_dryness`, Kind und Erwachsener gleich, keine Geschlechtsunterschiede. Unter
  `values` die sieben Spielwerte der Trockenheit (ADR-0034 Abschnitt 3):
  `core:body_dryness_offset` −0.942, `_curl` 0.75, `_slope` 1.30, `_age` 2.0 und `_male`
  −0.35 (Einheit `logit`), `_center` 55 und `_tempo` 10 (Einheit `year`).
- Die Pooldatei `data/pool/pool.bin` bleibt bytegleich, ihr `frozen_hash`
  `4864bcf40e824de2` ist der Hash der Definitionen ohne beide Erweiterungen, mit
  `map_version` 4. Die zwei Genorte kommen wie die des Temperaments beim Laden in die Genome
  (ADR-0034 Abschnitt 2). Hash der Definitionen `b037c9deff201479`.

## 5 (2026-11-17)

- Fünf Temperamentsmerkmale (Spezifikation 4.1, 4.2 und 6.2, ADR-0020 Abschnitt 1,
  ADR-0033 Abschnitt 2): `core:agreeableness`, `core:conscientiousness`,
  `core:extraversion`, `core:neuroticism`, `core:openness`. Jedes mit acht eigenen
  additiven autosomalen Genorten (`core:<merkmal>_01` bis `_08`), Effekt 1, Ladung +1,
  Klasse klein, Mutation in beide Richtungen. Die Orte liegen in freien Plätzen der
  Karte 4 nach der Regel aus ADR-0020 Abschnitt 1 (die Ordnungszahl zählt nur unter den
  fünf neuen Merkmalen); die alten Genorte behalten ihre Stellen. Kartenformat
  unverändert, `map_version` 5.
- Referenzwerte je Merkmal: `d_ref` 1 je Genort, `sigma_ref` 2, `mean_ref` 0. Das sind
  die exakten Parameter der Hardy-Weinberg-Verteilung bei p = 0.5
  (`Var(A) = 8 · 2p(1−p) = 4`), eine Ausnahme vom Messen.
- Kalibrierung: `h2` des Kindes 0.5, `c2` des Kindes 0.15, `h2` des Erwachsenen 0.5,
  `c2` des Erwachsenen 0.05 (Spezifikation 4.2). Geschlechtsunterschiede (Spezifikation
  6.2, Einheit Standardabweichung, Varianzverhältnis 1): Verträglichkeit −0.3,
  Gewissenhaftigkeit −0.1, Extraversion −0.1, Neurotizismus −0.4, Offenheit 0. Unter
  `values` je Achse `development_mean` 0 und `development_sd` 1 (Standardabweichungen),
  bis ein Messlauf sie ersetzt.
- Die Pooldatei `data/pool/pool.bin` bleibt bytegleich. Ihr `frozen_hash`
  `4864bcf40e824de2` ist der Hash der Definitionen ohne diese Erweiterung, mit
  `map_version` 4. Die vierzig Genorte kommen beim Laden in die Genome, nicht in die
  Datei.

## 4 (2026-10-19)

- Karte 4 aus ADR-0020, vom Vorlauf (`cargo xtask prerun`) aus der Vorlage der Karte 4
  (`tests/golden/map4_template`) eingefroren: 38 Merkmale, jedes mit gemessenem `sigma_ref`
  und `mean_ref`; 128 Genorte: die 121 additiven Genorte der Merkmale mit gemessenem `d_ref`,
  die fünf Krankheitsorte der Karte 3 unverändert an denselben Stellen, `core:mc1r` und
  `core:herc2`. Kartenformat 2. Die Kalibrierung wird die der Vorlage (Phänotypzeilen,
  Geschlechtsunterschied von `core:cognition`, Anfälligkeit der Schwellenkrankheit, benannte
  Merkmale); die Lebenslaufdaten bleiben.
- Messläufe: 40 000 Gründer, 750 Jahre, Welt-Seed `0x11b019a1`, Gründer-Tag
  `core:prerun_measure`. Lauf 1 auf der Vorlage (Daten-Hash des Vorlaufs `b17afdcd296175ac`),
  Lauf 2 auf der Karte von Lauf 1 (`fa6e3af2889ed345`), Lauf 3 auf der Karte von Lauf 2
  (`9359fc64627858f8`). Lauf 3 erfüllte die Stoppregel (`sigma_ref` von `core:growth`,
  `core:cognition`, `core:longevity`, `core:puberty` und `core:disease_liability` änderten
  sich um weniger als 2 Prozent); die Karte ist nach Lauf 3 eingefroren: `d_ref` ist die
  mittlere Dosis und `sigma_ref` die Streuung der Lebenden beim letzten Zensus. Die
  Wiederholung von Lauf 3 (2026-10-16) mit derselben Welt fügt `mean_ref` hinzu, den
  mittleren genetischen Wert aller in seinen letzten 30 Jahren Geborenen.
- Poollauf auf der eingefrorenen Karte: 40 000 Gründer, 750 Jahre, Welt-Seed `0x11b019a2`,
  Gründer-Tag `core:prerun_pool`. Er schrieb `data/pool/pool.bin` (Poolformat 2, 7 980 637
  Bytes, 20 859 Haushalte, 40 009 Personen, 409 Ungeborene, 79 836 Datensätze der Vorfahren,
  eingefroren in Sekunde 23 587 200 000), die Allelfrequenzen der sieben Orte des Pools
  (`data/pool/pool_frequencies.json`) und den Datensatz der Klemmquote seiner letzten zehn
  Zensus, Jahre 740 bis 749 (`data/pool/reference_census.json`).
- Hashes (xxh3-64): die Definitionen (Lebenslaufdaten, Kalibrierung, Karte)
  `c07b1d81763a3e32`, der `frozen_hash` des Pools; die Daten des Vorlaufs auf der
  eingefrorenen Karte `e96be897ae6984a0`, der `data_hash` des Pools; die Pooldatei
  `ac11f329cd5da77a`; der Daten-Hash der CLI, die Definitionen mit dem Pool und seinen
  Frequenzen, `b1f5c0054302363d`.

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
