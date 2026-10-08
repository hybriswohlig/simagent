//! ADR-0007: the definitions of the repository, their checks and their data hash.

mod common;

use serde_json::{Value, json};
use sim_core::data::{Data, DataError, Morgans, PerTransmission};

/// The data hash of the definitions of the repository, from `tools/data_reference.py`.
const DATA_HASH: u64 = 0xd5cc_b8c5_cd8b_30f4;

/// A change of the two files: calibration, genome map.
type Change = fn(&mut Value, &mut Value);

/// The check of definitions changed by `change`, as its error code or "ok".
fn checked(change: impl FnOnce(&mut Value, &mut Value)) -> String {
    let (mut calibration, mut genome_map) = common::files();
    change(&mut calibration, &mut genome_map);
    match common::definitions(calibration, genome_map) {
        Err(serde) => format!("serde: {serde}"),
        Ok(definitions) => match Data::new(definitions) {
            Ok(_) => "ok".to_owned(),
            Err(error) => error.to_string(),
        },
    }
}

fn locus(slot: u64) -> Value {
    json!({
        "id": format!("test:l{slot}"),
        "chromosome": "autosome",
        "segment": 1,
        "slot": slot,
        "action": "recessive",
        "mark": "large",
        "mutation": "both"
    })
}

/// A trait without includes (ADR-0008 section 1).
fn trait_(id: &str, sigma_ref: f64) -> Value {
    json!({"id": id, "sigma_ref": sigma_ref, "exclusive": false, "includes": []})
}

/// An additive locus on segment 2 at `slot` with these loads (ADR-0008 section 2).
fn additive(slot: u64, loads: Value) -> Value {
    json!({
        "id": format!("test:a{slot}"),
        "chromosome": "autosome",
        "segment": 2,
        "slot": slot,
        "action": {"additive": {"effect": 0.5, "d_ref": 1.0, "loads": loads}},
        "mark": "small",
        "mutation": "both"
    })
}

/// A load of a locus or an include of a trait.
fn on(id: &str, weight: f64) -> Value {
    json!({"trait": id, "weight": weight})
}

/// A map with the traits `test:a` and `test:b`, each with one additive locus.
fn with_traits(g: &mut Value) {
    g["traits"] = json!([trait_("test:a", 1.0), trait_("test:b", 2.0)]);
    let dummy = g["loci"][0].clone();
    g["loci"] = json!([
        dummy,
        additive(0, json!([on("test:a", 1.0)])),
        additive(1, json!([on("test:b", -1.0)]))
    ]);
}

#[test]
fn data_hash_is_pinned() {
    assert_eq!(common::data().hash(), DATA_HASH);
}

/// A map with traits, an include, an exclusive trait, two loads, additive and threshold
/// loci (ADR-0008): its data hash from `tools/data_reference.py tests/golden/data_traits`.
#[test]
fn a_map_with_traits_hashes_like_the_reference() {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    let definitions = common::definitions(calibration, genome_map).expect("definitions");
    let data = Data::new(definitions).expect("checked");
    assert_eq!(data.hash(), 0x1644_7a40_212e_298c);
}

#[test]
fn the_calibration_is_read() {
    let genetics = common::data().genetics();
    assert_eq!(genetics.map_length_autosome_female, Morgans(5.35));
    assert_eq!(genetics.map_length_autosome_male, Morgans(3.24));
    assert_eq!(genetics.map_length_x_female, Morgans(1.79));
    assert_eq!(genetics.mutation_rate, PerTransmission(5e-5));
    assert_eq!(genetics.mutation_rate_disease, PerTransmission(1e-6));
    assert_eq!(common::data().genome_map().loci.len(), 1);
}

#[test]
fn the_hash_follows_every_value() {
    let changed = |change: Change| {
        let (mut calibration, mut genome_map) = common::files();
        change(&mut calibration, &mut genome_map);
        let definitions = common::definitions(calibration, genome_map).expect("definitions");
        Data::new(definitions).expect("checked").hash()
    };
    let changes: [Change; 4] = [
        |c, _| c["values"]["core:map_length_x_female"]["value"] = json!(1.8),
        |_, g| g["map_version"] = json!(3),
        |_, g| g["loci"][0]["slot"] = json!(1),
        |_, g| with_traits(g),
    ];
    for change in changes {
        assert_ne!(changed(change), DATA_HASH);
    }
}

#[test]
fn genome_map_rejects() {
    let cases: [(Change, &str); 20] = [
        (|_, g| g["format_version"] = json!(1), "data.format_version"),
        (|_, g| g["format_version"] = json!(3), "data.format_version"),
        (|_, g| g["map_version"] = json!(0), "data.map_version"),
        // Positions: slot and segment within the chromosome type.
        (
            |_, g| g["loci"][0]["slot"] = json!(24),
            "data.locus_position",
        ),
        (
            |_, g| g["loci"][0]["segment"] = json!(8),
            "data.locus_position",
        ),
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("x");
                g["loci"][0]["slot"] = json!(16);
            },
            "data.locus_position",
        ),
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("y");
                g["loci"][0]["slot"] = json!(8);
            },
            "data.locus_position",
        ),
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("mt");
                g["loci"][0]["segment"] = json!(1);
            },
            "data.locus_position",
        ),
        // Order: strictly increasing by chromosome type, segment and slot.
        (
            |_, g| g["loci"] = json!([locus(3), locus(2)]),
            "data.locus_order",
        ),
        (
            |_, g| g["loci"] = json!([locus(2), locus(2)]),
            "data.locus_order",
        ),
        (
            |_, g| {
                let mut other = locus(3);
                other["id"] = json!("test:l2");
                g["loci"] = json!([locus(2), other]);
            },
            "data.locus_id",
        ),
        // Marks (B9, specification 2.4): disease loci mutate only to the alternative
        // allele, mendelian loci are marked large or disease.
        (
            |_, g| g["loci"][0]["mutation"] = json!("both"),
            "data.locus_mark",
        ),
        (
            |_, g| {
                g["loci"] = json!([{"id": "test:r", "chromosome": "autosome", "segment": 0,
                "slot": 0, "action": "recessive", "mark": "small", "mutation": "both"}])
            },
            "data.locus_mark",
        ),
        (|_, g| g["loci"][0]["chromosome"] = json!("w"), "serde"),
        // X and Y have one segment; the chromosome type comes first in the order.
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("x");
                g["loci"][0]["segment"] = json!(1);
            },
            "data.locus_position",
        ),
        (
            |_, g| {
                g["loci"][0]["chromosome"] = json!("y");
                g["loci"][0]["segment"] = json!(1);
            },
            "data.locus_position",
        ),
        (
            |_, g| {
                let mut on_x = locus(0);
                on_x["chromosome"] = json!("x");
                on_x["segment"] = json!(0);
                g["loci"] = json!([on_x, locus(5)]);
            },
            "data.locus_order",
        ),
        (
            |_, g| g["loci"][0]["mutation"] = json!("to_ref"),
            "data.locus_mark",
        ),
        (
            |_, g| {
                g["loci"] = json!([{"id": "test:d", "chromosome": "autosome", "segment": 0,
                "slot": 0, "action": "dominant", "mark": "small", "mutation": "both"}])
            },
            "data.locus_mark",
        ),
        (
            |_, g| {
                g["loci"] = json!([{"id": "test:d", "chromosome": "autosome", "segment": 0,
                "slot": 0, "action": "dominant", "mark": "small", "mutation": "to_alt"}])
            },
            "data.locus_mark",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // The cases are no accident of the repository's map: valid variants pass.
    assert_eq!(
        checked(|_, g| g["loci"] = json!([locus(2), locus(3)])),
        "ok"
    );
    assert_eq!(
        checked(
            |_, g| g["loci"] = json!([{"id": "test:d", "chromosome": "autosome",
            "segment": 0, "slot": 0, "action": "dominant", "mark": "large", "mutation": "both"}])
        ),
        "ok"
    );
    assert_eq!(
        checked(|_, g| {
            let mut on_x = locus(0);
            on_x["chromosome"] = json!("x");
            on_x["segment"] = json!(0);
            g["loci"] = json!([locus(5), on_x]);
        }),
        "ok"
    );
    assert_eq!(
        checked(|_, g| {
            g["loci"][0]["chromosome"] = json!("x");
            g["loci"][0]["slot"] = json!(15);
        }),
        "ok"
    );
}

/// ADR-0008 section 5: traits, weights and loads.
#[test]
fn traits_and_weights_reject() {
    let cases: [(Change, &str); 32] = [
        (
            |_, g| {
                with_traits(g);
                g["traits"] = json!([trait_("test:b", 1.0), trait_("test:a", 1.0)]);
            },
            "data.trait_order",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"] = json!([trait_("test:a", 1.0), trait_("test:a", 1.0)]);
            },
            "data.trait_order",
        ),
        // A trait may not share its data ID with a locus (the key is a draw occasion).
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["id"] = json!("core:dummy");
                g["loci"][1]["action"]["additive"]["loads"] = json!([on("core:dummy", 1.0)]);
            },
            "data.trait_key",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"][1]["sigma_ref"] = json!(0.0);
            },
            "data.trait_sigma",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"][1]["sigma_ref"] = json!(-1.0);
            },
            "data.trait_sigma",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["includes"] = json!([on("test:c", 1.0)]);
            },
            "data.trait_include",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["includes"] = json!([on("test:a", 1.0)]);
            },
            "data.trait_include",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["includes"] = json!([on("test:b", 1.0), on("test:b", 2.0)]);
            },
            "data.trait_include",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["includes"] = json!([on("test:b", 0.0)]);
            },
            "data.trait_include",
        ),
        // Includes stand in ascending order of their data IDs (ADR-0003 2.3).
        (
            |_, g| {
                with_traits(g);
                g["traits"] = json!([
                    trait_("test:a", 1.0),
                    trait_("test:b", 2.0),
                    trait_("test:c", 1.0)
                ]);
                g["traits"][2]["includes"] = json!([on("test:b", 1.0), on("test:a", 1.0)]);
            },
            "data.trait_include",
        ),
        // A locus may not count twice in one trait: directly and through an include.
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["includes"] = json!([on("test:b", 1.0)]);
                g["loci"][1]["action"]["additive"]["loads"] =
                    json!([on("test:a", 1.0), on("test:b", 1.0)]);
            },
            "data.trait_include",
        ),
        // Includes are one level deep.
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["includes"] = json!([on("test:b", 1.0)]);
                g["traits"][1]["includes"] = json!([on("test:a", 1.0)]);
            },
            "data.trait_include",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][2]["action"]["additive"]["loads"] = json!([on("test:a", 1.0)]);
            },
            "data.trait_unused",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["effect"] = json!(0.0);
            },
            "data.locus_weights",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["effect"] = json!(-0.5);
            },
            "data.locus_weights",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["d_ref"] = json!(2.5);
            },
            "data.locus_weights",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["d_ref"] = json!(-0.25);
            },
            "data.locus_weights",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["loads"] = json!([]);
            },
            "data.locus_weights",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"] = json!([
                    trait_("test:a", 1.0),
                    trait_("test:b", 1.0),
                    trait_("test:c", 1.0)
                ]);
                g["loci"][1]["action"]["additive"]["loads"] =
                    json!([on("test:a", 1.0), on("test:b", 1.0), on("test:c", 1.0)]);
            },
            "data.locus_weights",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["loads"] = json!([on("test:a", 0.0)]);
            },
            "data.locus_weights",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["loads"] =
                    json!([on("test:a", 1.0), on("test:a", 2.0)]);
            },
            "data.locus_weights",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["loads"] = json!([on("test:c", 1.0)]);
            },
            "data.locus_load",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["exclusive"] = json!(true);
                g["loci"][1]["action"]["additive"]["loads"] =
                    json!([on("test:a", 1.0), on("test:b", 1.0)]);
            },
            "data.load_exclusive",
        ),
        // Including an exclusive trait would let its loci act on another one.
        (
            |_, g| {
                with_traits(g);
                g["traits"][1]["exclusive"] = json!(true);
                g["traits"][0]["includes"] = json!([on("test:b", 1.0)]);
            },
            "data.load_exclusive",
        ),
        (
            |_, g| {
                with_traits(g);
                g["loci"][2]["chromosome"] = json!("x");
                g["loci"][2]["segment"] = json!(0);
            },
            "data.locus_additive",
        ),
        // Finite inputs whose genetic value would be infinite or NaN (ADR-0008 section 3).
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["effect"] = json!(1e308);
                g["loci"][1]["action"]["additive"]["loads"] = json!([on("test:a", 1e308)]);
            },
            "data.trait_bounds",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["sigma_ref"] = json!(1e-320);
            },
            "data.trait_bounds",
        ),
        (
            |_, g| {
                with_traits(g);
                g["traits"][0]["includes"] = json!([on("test:b", 1e308)]);
                g["loci"][2]["action"]["additive"]["effect"] = json!(1e308);
            },
            "data.trait_bounds",
        ),
        // Each term finite, their sum not: the bound folds the magnitudes of all terms.
        (
            |_, g| {
                with_traits(g);
                let huge = json!({"effect": 1e308, "d_ref": 1.0, "loads": [on("test:a", 1.0)]});
                g["loci"][1]["action"]["additive"] = huge.clone();
                g["loci"][2]["action"]["additive"] = huge;
            },
            "data.trait_bounds",
        ),
        // A threshold locus is weighted like an additive one: autosomes only.
        (
            |_, g| {
                with_traits(g);
                let weights = g["loci"][2]["action"]["additive"].clone();
                g["loci"][2]["action"] = json!({"threshold": weights});
                g["loci"][2]["chromosome"] = json!("x");
                g["loci"][2]["segment"] = json!(0);
            },
            "data.locus_additive",
        ),
        // A term that is 0 at every dose: the product underflows.
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"]["additive"]["effect"] = json!(1e-200);
                g["loci"][1]["action"]["additive"]["loads"] = json!([on("test:a", 1e-200)]);
            },
            "data.trait_bounds",
        ),
        // The weights belong to the action: an additive locus without them is no locus.
        (
            |_, g| {
                with_traits(g);
                g["loci"][1]["action"] = json!("additive");
            },
            "serde",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // Valid variants: two loads, an include, an exclusive trait, a threshold locus.
    let valid: [Change; 4] = [
        |_, g| with_traits(g),
        |_, g| {
            with_traits(g);
            g["loci"][1]["action"]["additive"]["loads"] =
                json!([on("test:a", 1.0), on("test:b", 0.5)]);
        },
        |_, g| {
            with_traits(g);
            g["traits"][0]["includes"] = json!([on("test:b", -0.3)]);
            g["traits"][0]["exclusive"] = json!(true);
        },
        |_, g| {
            with_traits(g);
            let weights = g["loci"][2]["action"]["additive"].clone();
            g["loci"][2]["action"] = json!({"threshold": weights});
        },
    ];
    for change in valid {
        assert_eq!(checked(change), "ok");
    }
    // An include whose weight makes its terms underflow to 0 is allowed: the rule that a
    // term must not vanish holds for loads (ADR-0008 section 3), and 0 is finite.
    assert_eq!(
        checked(|_, g| {
            with_traits(g);
            g["loci"][2]["action"]["additive"]["effect"] = json!(1e-200);
            g["loci"][2]["action"]["additive"]["d_ref"] = json!(0.0);
            g["loci"][2]["action"]["additive"]["loads"] = json!([on("test:b", 1.0)]);
            g["traits"][0]["includes"] = json!([on("test:b", 1e-200)]);
        }),
        "ok"
    );
    // A trait that only includes others is used; two includes in ascending order may
    // share a locus.
    assert_eq!(
        checked(|_, g| {
            with_traits(g);
            g["traits"] = json!([
                trait_("test:a", 1.0),
                trait_("test:b", 2.0),
                trait_("test:c", 1.0)
            ]);
            g["traits"][2]["includes"] = json!([on("test:a", 1.0), on("test:b", 0.5)]);
            g["loci"][1]["action"]["additive"]["loads"] =
                json!([on("test:a", 1.0), on("test:b", 1.0)]);
        }),
        "ok"
    );
}

/// A row of phenotype weights (ADR-0009 section 1), without its text.
fn row(t: &str, weights: [f64; 4]) -> Value {
    let fraction = |value: f64| json!({"value": value, "unit": "fraction"});
    json!({"trait": t, "h2_child": fraction(weights[0]), "c2_child": fraction(weights[1]),
        "h2_adult": fraction(weights[2]), "c2_adult": fraction(weights[3])})
}

/// ADR-0009 section 2: the phenotype weights of the calibration.
#[test]
fn phenotype_weights_reject() {
    let cases: [(Change, &str); 9] = [
        (
            |c, g| {
                with_traits(g);
                c["traits"] = json!([row("test:c", [0.5, 0.1, 0.5, 0.1])]);
            },
            "data.calibration_unknown",
        ),
        (
            |c, g| {
                with_traits(g);
                c["traits"] = json!([
                    row("test:b", [0.5, 0.1, 0.5, 0.1]),
                    row("test:a", [0.5, 0.1, 0.5, 0.1])
                ]);
            },
            "data.calibration_unknown",
        ),
        (
            |c, g| {
                with_traits(g);
                c["traits"] = json!([
                    row("test:a", [0.5, 0.1, 0.5, 0.1]),
                    row("test:a", [0.5, 0.1, 0.5, 0.1])
                ]);
            },
            "data.calibration_unknown",
        ),
        (
            |c, g| {
                with_traits(g);
                c["traits"] = json!([row("test:a", [0.5, 0.1, 0.5, 0.1])]);
                c["traits"][0]["c2_adult"]["unit"] = json!("morgan");
            },
            "data.calibration_unit",
        ),
        (
            |c, g| {
                with_traits(g);
                c["traits"] = json!([row("test:a", [1.5, 0.0, 0.5, 0.1])]);
            },
            "data.calibration_range",
        ),
        (
            |c, g| {
                with_traits(g);
                c["traits"] = json!([row("test:a", [0.5, -0.125, 0.5, 0.1])]);
            },
            "data.calibration_range",
        ),
        (
            |c, g| {
                with_traits(g);
                c["traits"] = json!([row("test:a", [0.8, 0.25, 0.5, 0.1])]);
            },
            "data.calibration_range",
        ),
        (
            |c, g| {
                with_traits(g);
                c["traits"] = json!([row("test:a", [0.5, 0.1, 0.75, 0.375])]);
            },
            "data.calibration_range",
        ),
        // The weights are calibration values with a unit: a bare number is no value.
        (
            |c, g| {
                with_traits(g);
                c["traits"] = json!([row("test:a", [0.5, 0.1, 0.5, 0.1])]);
                c["traits"][0]["h2_child"] = json!(0.5);
            },
            "serde",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // A trait with a phenotype keeps the bound of its genetic value at most 2^1022
    // (ADR-0009 section 2); without a row only finiteness counts (ADR-0008).
    let huge = |c: &mut Value, g: &mut Value, with_row: bool| {
        with_traits(g);
        g["loci"][1]["action"]["additive"]["effect"] = json!(1e308);
        if with_row {
            c["traits"] = json!([row("test:a", [0.5, 0.1, 0.5, 0.1])]);
        }
    };
    assert!(checked(|c, g| huge(c, g, true)).starts_with("data.trait_bounds"));
    assert_eq!(checked(|c, g| huge(c, g, false)), "ok");
    // Valid: c2 = 0, h2 + c2 = 1 exactly, a trait without a row, both traits with rows.
    let valid: [Change; 3] = [
        |c, g| {
            with_traits(g);
            c["traits"] = json!([row("test:a", [0.6, 0.0, 0.6, 0.0])]);
        },
        |c, g| {
            with_traits(g);
            c["traits"] = json!([row("test:b", [0.75, 0.25, 0.5, 0.5])]);
        },
        |c, g| {
            with_traits(g);
            c["traits"] = json!([
                row("test:a", [0.5, 0.25, 0.75, 0.05]),
                row("test:b", [0.4, 0.1, 0.4, 0.0])
            ]);
        },
    ];
    for change in valid {
        assert_eq!(checked(change), "ok");
    }
}

#[test]
fn calibration_rejects() {
    let cases: [(Change, &str); 11] = [
        (|c, _| c["format_version"] = json!(0), "data.format_version"),
        (|c, _| c["format_version"] = json!(1), "data.format_version"),
        (
            |c, _| {
                c["values"]
                    .as_object_mut()
                    .expect("values")
                    .remove("core:mutation_rate");
            },
            "data.calibration_missing",
        ),
        (
            |c, _| c["values"]["core:extra"] = json!({"value": 1.0, "unit": "morgan"}),
            "data.calibration_unknown",
        ),
        (
            |c, _| c["values"]["core:mutation_rate"]["unit"] = json!("morgan"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:mutation_rate"]["value"] = json!(0.0),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:map_length_autosome_male"]["value"] = json!(30.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:mutation_rate"]["value"] = json!(-1e-5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:mutation_rate"]["value"] = json!(1.000_001),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:map_length_autosome_male"]["value"] = json!(0.0),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:map_length_autosome_male"]["value"] = json!(-1.0),
            "data.calibration_range",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // A rate may be 1 at most.
    assert_eq!(
        checked(|c, _| c["values"]["core:mutation_rate"]["value"] = json!(1.0)),
        "ok"
    );
    // The upper limit of a map length is the largest mean of `poisson`.
    assert_eq!(
        checked(|c, _| c["values"]["core:map_length_autosome_male"]["value"] = json!(30.0)),
        "ok"
    );
    assert_eq!(
        DataError::CalibrationRange.to_string(),
        "data.calibration_range"
    );
}
