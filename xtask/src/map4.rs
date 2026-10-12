//! The template of the base map 4 (ADR-0020 section 1, design note of the template): the
//! traits with `σ_start`, the new additive loci with `d_ref` = 1 placed by the rule, MC1R
//! and HERC2, and the calibration with the phenotype rows, the named traits, the sex
//! difference of cognition and the trait of liability. The pre-run starts from it; the
//! repository keeps map 3 until the frozen map replaces it.

use std::collections::BTreeMap;
use std::path::Path;

use crate::Result;
use crate::ordered::Json;

/// A trait of the template: its own additive loci and the factor it includes.
struct TraitSpec {
    id: &'static str,
    own: u32,
    include: Option<(&'static str, f64)>,
    exclusive: bool,
}

const GROWTH: (&str, f64) = ("core:growth", 0.15);
const ANDROGEN: (&str, f64) = ("core:androgen", 0.50);
const ESTROGEN: (&str, f64) = ("core:estrogen", 0.70);

/// Every trait of map 4 (ADR-0020 section 1), in any order; `traits` sorts them.
fn specs() -> Vec<TraitSpec> {
    let plain = |id, own| TraitSpec {
        id,
        own,
        include: None,
        exclusive: false,
    };
    let with = |id, own, include| TraitSpec {
        id,
        own,
        include: Some(include),
        exclusive: false,
    };
    let mut all = vec![
        plain("core:growth", 16),
        TraitSpec {
            exclusive: true,
            ..plain("core:pigment", 16)
        },
        plain("core:cognition", 16),
        plain("core:androgen", 8),
        plain("core:longevity", 8),
        plain("core:disease_liability", 8),
        plain("core:puberty", 6),
        plain("core:estrogen", 2),
    ];
    for face in [
        "core:face_01",
        "core:face_02",
        "core:face_03",
        "core:face_04",
        "core:face_05",
        "core:face_06",
        "core:face_07",
        "core:face_08",
        "core:face_09",
        "core:face_10",
        "core:face_11",
        "core:face_12",
        "core:leg_ratio",
        "core:head_size",
        "core:chest_depth",
    ] {
        all.push(with(face, 1, GROWTH));
    }
    for (id, own) in [
        ("core:muscle", 2),
        ("core:shoulder_width", 1),
        ("core:fat_distribution", 1),
        ("core:beard", 2),
        ("core:body_hair", 1),
        ("core:bald", 2),
    ] {
        all.push(with(id, own, ANDROGEN));
    }
    all.push(with("core:hip_width", 1, ESTROGEN));
    all.push(with("core:breast", 2, ESTROGEN));
    for id in [
        "core:hair_darkness",
        "core:curl",
        "core:freckle",
        "core:hair_density",
        "core:grey",
        "core:skin_age",
        "core:body_fat",
    ] {
        all.push(plain(id, 2));
    }
    all.sort_by_key(|spec| spec.id);
    all
}

/// `σ_start = √(n_own / 2 + w² · n_s / 2)` (ADR-0020 section 3): every locus additive with
/// effect 1, load 1 and frequency 0.5, so its genic variance is 1/2.
fn sigma_start(spec: &TraitSpec, own_of: &BTreeMap<&str, u32>) -> f64 {
    let own = f64::from(spec.own) * 0.5;
    let included = spec.include.map_or(0.0, |(factor, weight)| {
        weight * weight * f64::from(own_of[factor]) * 0.5
    });
    sim_math::sqrt(own + included)
}

/// The slots of an autosome segment (ADR-0001: 8 segments of 24).
const SEGMENTS: u32 = 8;
const SLOTS: u32 = 24;

/// The places of the new additive loci (ADR-0020 section 1): per trait with own loci in
/// ascending ID (ordinal `o`), locus `i` in segment `(o · 3 + i) mod 8`, at the free slot
/// with the largest smallest distance to the trait's loci already in that segment, else
/// the smallest free slot; ties to the smaller slot. `taken` holds the occupied
/// (segment, slot) pairs. Returns per trait its places in order of `i`.
fn place(
    specs: &[TraitSpec],
    taken: &mut std::collections::BTreeSet<(u32, u32)>,
) -> Result<Vec<Vec<(u32, u32)>>> {
    let mut all = Vec::new();
    let with_loci = specs.iter().filter(|spec| spec.own > 0);
    for (o, spec) in (0_u32..).zip(with_loci) {
        let mut mine: Vec<(u32, u32)> = Vec::new();
        for i in 0..spec.own {
            let segment = (o * 3 + i) % SEGMENTS;
            let own_here: Vec<u32> = mine
                .iter()
                .filter(|(s, _)| *s == segment)
                .map(|(_, slot)| *slot)
                .collect();
            let free = (0..SLOTS).filter(|slot| !taken.contains(&(segment, *slot)));
            let best = if own_here.is_empty() {
                free.min()
            } else {
                // The largest smallest distance; `max_by_key` keeps the last of equals, so
                // compare (distance, reversed slot) to keep the smaller slot.
                free.max_by_key(|slot| {
                    let nearest = own_here.iter().map(|s| s.abs_diff(*slot)).min();
                    (nearest, std::cmp::Reverse(*slot))
                })
            };
            let slot = best.ok_or_else(|| format!("segment {segment} is full ({})", spec.id))?;
            taken.insert((segment, slot));
            mine.push((segment, slot));
        }
        all.push(mine);
    }
    Ok(all)
}

fn read(root: &Path, path: &str) -> Result<Json> {
    let text = std::fs::read_to_string(root.join(path))
        .map_err(|error| format!("cannot read {path}: {error}"))?;
    Json::parse(&text).map_err(|error| format!("{path}: {error}"))
}

/// The position of a locus as the map orders it: autosomes by segment and slot, then X,
/// Y, mt.
fn position(locus: &Json) -> (u32, u64, u64) {
    let rank = match locus.get("chromosome").and_then(Json::as_str) {
        Some("autosome") => 0,
        Some("x") => 1,
        Some("y") => 2,
        _ => 3,
    };
    let at = |key: &str| locus.get(key).and_then(Json::as_u64).unwrap_or(0);
    (rank, at("segment"), at("slot"))
}

/// A locus of the template in the field order of the data files.
fn locus(id: &str, segment: u32, slot: u32, action: Json, mark: &str, description: String) -> Json {
    Json::object(vec![
        ("id", Json::str(id)),
        ("chromosome", Json::str("autosome")),
        ("segment", Json::int(u64::from(segment))),
        ("slot", Json::int(u64::from(slot))),
        ("action", action),
        ("mark", Json::str(mark)),
        ("mutation", Json::str("both")),
        ("description", Json::str(description)),
    ])
}

/// The template (calibration, genome map) from the repository's files.
pub fn template(root: &Path) -> Result<(Json, Json)> {
    let map3 = read(root, "data/genetics/genome_map.json")?;
    let mut calibration = read(root, "data/calibration/genetics.json")?;
    let specs = specs();
    let own_of: BTreeMap<&str, u32> = specs.iter().map(|spec| (spec.id, spec.own)).collect();
    let mut loci: Vec<Json> = map3
        .get("loci")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    let mut taken = std::collections::BTreeSet::new();
    for old in &loci {
        if old.get("chromosome").and_then(Json::as_str) == Some("autosome") {
            let at = |key: &str| {
                old.get(key)
                    .and_then(Json::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| format!("map 3: a locus without {key}"))
            };
            taken.insert((at("segment")?, at("slot")?));
        }
    }
    for (id, segment) in [("core:mc1r", 1), ("core:herc2", 3)] {
        taken.insert((segment, 12));
        loci.push(locus(
            id,
            segment,
            12,
            Json::str("recessive"),
            "large",
            texts::pigment_locus(id),
        ));
    }
    let places = place(&specs, &mut taken)?;
    for (spec, mine) in specs.iter().filter(|spec| spec.own > 0).zip(&places) {
        for (i, (segment, slot)) in (1_u32..).zip(mine) {
            let action = Json::object(vec![(
                "additive",
                Json::object(vec![
                    ("effect", Json::float(1.0)),
                    ("d_ref", Json::float(1.0)),
                    (
                        "loads",
                        Json::Array(vec![Json::object(vec![
                            ("trait", Json::str(spec.id)),
                            ("weight", Json::float(1.0)),
                        ])]),
                    ),
                ]),
            )]);
            loci.push(locus(
                &format!("{}_{i:02}", spec.id),
                *segment,
                *slot,
                action,
                "small",
                texts::additive_locus(spec.id, i),
            ));
        }
    }
    loci.sort_by_key(position);
    let traits: Vec<Json> = specs
        .iter()
        .map(|spec| {
            let includes = spec
                .include
                .iter()
                .map(|(factor, weight)| {
                    Json::object(vec![
                        ("trait", Json::str(*factor)),
                        ("weight", Json::float(*weight)),
                    ])
                })
                .collect();
            Json::object(vec![
                ("id", Json::str(spec.id)),
                ("sigma_ref", Json::float(sigma_start(spec, &own_of))),
                ("exclusive", Json::Bool(spec.exclusive)),
                ("includes", Json::Array(includes)),
                ("description", Json::str(texts::trait_description(spec))),
            ])
        })
        .collect();
    let map = Json::object(vec![
        (
            "format_version",
            map3.get("format_version").cloned().unwrap_or(Json::Null),
        ),
        ("map_version", Json::int(4)),
        ("traits", Json::Array(traits)),
        ("loci", Json::Array(loci)),
    ]);
    calibration.set(
        "traits",
        Json::Array(
            specs
                .iter()
                .filter(|spec| spec.id != "core:longevity")
                .map(|spec| phenotype_row(spec.id))
                .collect(),
        ),
    );
    calibration.set(
        "sex_differences",
        Json::Array(vec![texts::cognition_difference()]),
    );
    for (name, value) in [
        ("cognition_trait", "core:cognition"),
        ("height_trait", "core:growth"),
        ("puberty_trait", "core:puberty"),
        ("longevity_trait", "core:longevity"),
    ] {
        calibration.set(name, Json::str(value));
    }
    if let Some(diseases) = calibration.get_mut("diseases").and_then(Json::as_array_mut) {
        for disease in diseases {
            if disease.get("code").and_then(Json::as_str) == Some("core:disease_liability")
                && let Some(threshold) = disease
                    .get_mut("carrier")
                    .and_then(|carrier| carrier.get_mut("threshold"))
            {
                threshold.set("liability", Json::str("core:disease_liability"));
            }
        }
    }
    Ok((calibration, map))
}

/// The phenotype weights of a trait (h² and c² of the child, then of the adult;
/// ADR-0020 section 1).
fn weights(id: &str) -> [f64; 4] {
    match id {
        "core:growth" => [0.80, 0.05, 0.80, 0.05],
        "core:pigment" => [0.80, 0.0, 0.80, 0.0],
        "core:cognition" => [0.50, 0.25, 0.75, 0.05],
        "core:puberty" => [0.60, 0.10, 0.60, 0.10],
        "core:body_fat" => [0.50, 0.10, 0.50, 0.10],
        face if face.starts_with("core:face_") => [0.70, 0.0, 0.70, 0.0],
        _ => [0.60, 0.0, 0.60, 0.0],
    }
}

fn phenotype_row(id: &str) -> Json {
    let [h2_child, c2_child, h2_adult, c2_adult] = weights(id);
    Json::object(vec![
        ("trait", Json::str(id)),
        ("h2_child", texts::weight("h2_child", h2_child)),
        ("c2_child", texts::weight("c2_child", c2_child)),
        ("h2_adult", texts::weight("h2_adult", h2_adult)),
        ("c2_adult", texts::weight("c2_adult", c2_adult)),
    ])
}

/// The two files of the template as text, in the layout of the data files.
pub fn texts_of(root: &Path) -> Result<(String, String)> {
    let (calibration, map) = template(root)?;
    Ok((calibration.pretty()?, map.pretty()?))
}

/// Writes the template into `dir` (`calibration/genetics.json`, `genetics/genome_map.json`).
pub fn write(root: &Path, dir: &Path) -> Result {
    let (calibration, map) = texts_of(root)?;
    for (path, text) in [
        ("calibration/genetics.json", calibration),
        ("genetics/genome_map.json", map),
    ] {
        let file = dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        std::fs::write(&file, text)
            .map_err(|error| format!("cannot write {}: {error}", file.display()))?;
    }
    Ok(())
}

/// The texts of the template (descriptions, sources and notes); fixed so that the
/// reference writes the same.
mod texts {
    use super::TraitSpec;
    use crate::ordered::Json;

    pub fn additive_locus(t: &str, i: u32) -> String {
        format!("Additive locus {i} of {t} (ADR-0020 section 1).")
    }

    pub fn pigment_locus(id: &str) -> String {
        let name = if id == "core:mc1r" { "MC1R" } else { "HERC2" };
        format!(
            "{name}, a large recessive pigment locus (genetics specification 10.4, ADR-0020 section 1)."
        )
    }

    pub fn trait_description(spec: &TraitSpec) -> String {
        let loci = if spec.own == 1 { "locus" } else { "loci" };
        let include = spec.include.map_or(String::new(), |(factor, weight)| {
            format!(", including {factor} with weight {weight}")
        });
        let exclusive = if spec.exclusive { ", exclusive" } else { "" };
        format!(
            "Trait of the template of map 4 with {} own additive {loci}{include}{exclusive}; sigma_ref is its start value of the pre-run (ADR-0020 sections 1 and 3).",
            spec.own
        )
    }

    fn number(value: f64, unit: &str, source: &str, note: String) -> Json {
        Json::object(vec![
            ("value", Json::float(value)),
            ("unit", Json::str(unit)),
            ("source", Json::str(source)),
            ("as_of", Json::str("2026-10-12")),
            ("note", Json::str(note)),
        ])
    }

    pub fn weight(name: &str, value: f64) -> Json {
        let (h_or_c, age) = name.split_once('_').unwrap_or((name, ""));
        let age = if age == "child" {
            "the child"
        } else {
            "the adult"
        };
        number(
            value,
            "fraction",
            "ADR-0020 section 1 (genetics specification 4.2 and 10.7)",
            format!("{h_or_c} of {age}, template of map 4"),
        )
    }

    pub fn cognition_difference() -> Json {
        Json::object(vec![
            ("trait", Json::str("core:cognition")),
            (
                "shift",
                number(
                    0.0,
                    "standard_deviation",
                    "docs/genetik-spezifikation.md 6.2 (K6.2b), Deary et al. (2003)",
                    "no shift of men against women, template of map 4".to_owned(),
                ),
            ),
            (
                "variance_ratio",
                number(
                    1.12,
                    "ratio",
                    "docs/genetik-spezifikation.md 6.2 (K6.2c), Deary et al. (2003)",
                    "variance of men over women, template of map 4".to_owned(),
                ),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the workspace")
            .to_path_buf()
    }

    /// The template is the reference's (`tools/map4_reference.py`, the golden
    /// `tests/golden/map4_template`) byte for byte: the placement, `σ_start`, the rows, the
    /// texts and the layout of the data files (keys in their order; review).
    #[test]
    fn the_template_is_the_references() {
        let (calibration, map) = texts_of(&root()).expect("a template");
        let golden = |path: &str| {
            std::fs::read_to_string(root().join("tests/golden/map4_template").join(path))
                .expect("golden")
        };
        assert_eq!(map, golden("genetics/genome_map.json"));
        assert_eq!(calibration, golden("calibration/genetics.json"));
    }

    /// ADR-0020 section 1: 128 loci (121 additive, 126 autosomal), 38 traits; the growth
    /// includers' `σ_start` is one ulp below `√0.68` (0.15 · 0.15 rounds).
    #[test]
    fn the_template_has_the_counts_of_the_adr() {
        let (_, map) = template(&root()).expect("a template");
        let loci = map.get("loci").and_then(Json::as_array).expect("loci");
        let additive = loci
            .iter()
            .filter(|l| l.get("action").and_then(|a| a.get("additive")).is_some());
        let autosomal = loci
            .iter()
            .filter(|l| l.get("chromosome").and_then(Json::as_str) == Some("autosome"));
        assert_eq!(
            (loci.len(), additive.count(), autosomal.count()),
            (128, 121, 126)
        );
        let traits = map.get("traits").and_then(Json::as_array).expect("traits");
        assert_eq!(traits.len(), 38);
        let face = traits
            .iter()
            .find(|t| t.get("id").and_then(Json::as_str) == Some("core:face_01"))
            .expect("a face");
        assert_eq!(
            face.get("sigma_ref"),
            Some(&Json::float(f64::from_bits(0x3fea_634b_d77f_e1a4)))
        );
    }
}
