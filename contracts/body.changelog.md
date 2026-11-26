# Änderungen am Körper-Vertrag

Ein Abschnitt pro `contract_version` (ADR-0005 Abschnitt 7), der neueste oben, jeweils
mit den Einträgen Neu, Geändert und Stillgelegt.

## 0.1.3

- Neu: nichts.
- Geändert: die Haartabelle in der Zeile Eumelanin 0,25 ist bei Phäomelanin 0, 0,25 und 0,5
  `#a09078`, `#ae946c` und `#b08454` statt `#8c8171`, `#8a7764` und `#886c55`, damit
  Dunkelblond und Hellbraun golden statt mausbraun wirken; Kupfer und die übrigen Zeilen
  bleiben (Entscheidung Grok vom 2026-11-26 nach den Figuren).
- Stillgelegt: nichts.

## 0.1.2

- Neu: `hair_strand_thickness` (Haarstärke, Standard 0,5) und `hair_dryness` (Trockenheit,
  Standard 0,3) in der Gruppe `hair` nach `hair_curl`, Einheit `fraction`, Bereich 0 bis 1,
  `stable` (ADR-0034 Abschnitt 3).
- Geändert: nichts.
- Stillgelegt: nichts.

## 0.1.1

- Neu: nichts.
- Geändert: die Haartabelle an der roten Ecke (Eumelanin 0, Phäomelanin 1) ist `#ee5a28`
  statt `#dc9043`, damit rotes Haar rot statt kupfern aussieht (Grok Runde 39, R1 c, nach
  der Begutachtung des Bogens der Passbilder).
- Stillgelegt: nichts.

## 0.1.0

- Neu: die 44 Parameter des Startstands aus ADR-0005 Abschnitt 4 und die Melanintabelle
  für Haut, Haar und Iris mit Spielwerten (die Stilprobe justiert sie).
- Geändert: nichts.
- Stillgelegt: nichts.
