//! ADR-0007: the definitions of the map 3 fixture (`tests/golden/map3`, with the repository's
//! life course data), their checks and their data hash.

mod common;

use serde_json::{Value, json};
use sim_core::data::{
    Calibrated, Carrier, Data, DataError, Effect, Morgans, Onset, PerTransmission, ShockKind,
    ShockTemplate,
};
use sim_core::data_id::DataId;

/// The data hash of the definitions of the repository, from `tools/data_reference.py`.
const DATA_HASH: u64 = 0x1ccd_002b_fc5f_6185;

/// A change of the two files: calibration, genome map.
type Change = fn(&mut Value, &mut Value);

/// The check of definitions changed by `change`, as its error code or "ok"; without the
/// diseases, whose loci the changes of the map would take away (`checked_diseases` keeps
/// them).
fn checked(change: impl FnOnce(&mut Value, &mut Value)) -> String {
    checked_diseases(|c, g| {
        c["diseases"] = json!([]);
        change(c, g);
    })
}

/// The check of the definitions of the repository, diseases included, changed by `change`.
fn checked_diseases(change: impl FnOnce(&mut Value, &mut Value)) -> String {
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
    assert_eq!(data.hash(), 0x96ec_ba88_f9e4_f738);
}

#[test]
fn the_calibration_is_read() {
    let genetics = common::data().genetics();
    assert_eq!(genetics.map_length_autosome_female, Morgans(5.35));
    assert_eq!(genetics.map_length_autosome_male, Morgans(3.24));
    assert_eq!(genetics.map_length_x_female, Morgans(1.79));
    assert_eq!(genetics.mutation_rate, PerTransmission(5e-5));
    assert_eq!(genetics.mutation_rate_disease, PerTransmission(1e-6));
    let ids: Vec<String> = common::data()
        .genome_map()
        .loci
        .iter()
        .map(|locus| locus.id.as_str().to_owned())
        .collect();
    assert_eq!(
        ids,
        [
            "core:disease_ar",
            "core:disease_ad_late",
            "core:disease_ad_partial",
            "core:disease_xr",
            "core:disease_mt"
        ]
    );
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
        |_, g| g["map_version"] = json!(4),
        |_, g| g["loci"][0]["slot"] = json!(1),
        |c, g| {
            with_traits(g);
            c["diseases"] = json!([]);
        },
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
    // The last slot of X: the map's X locus moved there.
    assert_eq!(checked(|_, g| g["loci"][3]["slot"] = json!(15)), "ok");
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
                g["traits"][0]["id"] = json!("core:disease_ar");
                g["loci"][1]["action"]["additive"]["loads"] = json!([on("core:disease_ar", 1.0)]);
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
    let cases: [(Change, &str); 11] = [
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
        // The edges of 0 to 1: the next number above 1 and the next below 0.
        (
            |c, g| {
                with_traits(g);
                let above = f64::from_bits(1.0_f64.to_bits() + 1);
                c["traits"] = json!([row("test:a", [above, 0.0, 0.5, 0.1])]);
            },
            "data.calibration_range",
        ),
        (
            |c, g| {
                with_traits(g);
                let below = -f64::from_bits(1);
                c["traits"] = json!([row("test:a", [0.5, 0.1, 0.5, below])]);
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
    // (ADR-0009 section 2); without a row only finiteness counts (ADR-0008). With d_ref 1,
    // weight 1 and sigma_ref 1 the bound is the effect itself.
    let limit = f64::from_bits(2045_u64 << 52);
    let just_above = f64::from_bits((2045_u64 << 52) + 1);
    let bounded = |effect: f64, with_row: bool| {
        let (mut c, mut g) = common::files();
        with_traits(&mut g);
        c["diseases"] = json!([]);
        g["traits"][0]["sigma_ref"] = json!(1.0);
        g["loci"][1]["action"]["additive"]["effect"] = json!(effect);
        if with_row {
            c["traits"] = json!([row("test:a", [0.5, 0.1, 0.5, 0.1])]);
        }
        match Data::new(common::definitions(c, g).expect("definitions")) {
            Ok(_) => "ok".to_owned(),
            Err(error) => error.to_string(),
        }
    };
    assert_eq!(bounded(limit, true), "ok");
    assert_eq!(bounded(just_above, true), "data.trait_bounds");
    assert_eq!(bounded(just_above, false), "ok");
    // Valid: c2 = 0, h2 + c2 = 1 exactly, a trait without a row, both traits with rows.
    let valid: [Change; 4] = [
        |c, g| {
            with_traits(g);
            c["traits"] = json!([row("test:a", [1.0, 0.0, 1.0, 0.0])]);
        },
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

/// ADR-0010 section 2: the values of the mean growth curve.
#[test]
fn growth_values_reject() {
    let set = |c: &mut Value, key: &str, value: f64| c["values"][key]["value"] = json!(value);
    let cases: [(Change, &str); 9] = [
        (
            |c, _| c["values"]["core:growth_theta_male"]["unit"] = json!("centimeter"),
            "data.calibration_unit",
        ),
        // The medians rise, and by less in the second year than in the first.
        (
            |c, _| c["values"]["core:length_12_months_female"]["value"] = json!(49.0),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:length_24_months_male"]["value"] = json!(75.0),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:length_24_months_male"]["value"] = json!(101.7),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:growth_height_theta_female"]["value"] = json!(167.3),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:growth_s0_male"]["value"] = json!(0.9655),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:growth_theta_male"]["value"] = json!(30.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:height_adult_male"]["value"] = json!(300.5),
            "data.calibration_range",
        ),
        // Above 10 per year an exponent of the curve could underflow and the height
        // become infinite (ADR-0010 section 2).
        (
            |c, _| c["values"]["core:growth_s1_male"]["value"] = json!(10.5),
            "data.calibration_range",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // The limits themselves are allowed, and a second year of growth just below the first.
    let (mut c, g) = common::files();
    set(&mut c, "core:growth_theta_female", 30.0);
    set(&mut c, "core:growth_s1_female", 10.0);
    set(&mut c, "core:length_24_months_male", 101.6);
    assert!(Data::new(common::definitions(c, g).expect("definitions")).is_ok());
}

/// ADR-0012 section 2: the spreads, `γ` and the corner of the individual curve.
#[test]
fn individual_curve_values_reject() {
    let set = |c: &mut Value, key: &str, value: f64| c["values"][key]["value"] = json!(value);
    let cases: [(Change, &str); 8] = [
        (
            |c, _| c["values"]["core:growth_gamma"]["unit"] = json!("per_year"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:height_sd_male"]["unit"] = json!("year"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:growth_theta_sd_female"]["unit"] = json!("centimeter"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:height_sd_male"]["value"] = json!(0.0),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:height_sd_female"]["value"] = json!(300.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:growth_theta_sd_female"]["value"] = json!(5.01),
            "data.calibration_range",
        ),
        // The corner `z_h = z_p = −6`: 167.3 − 6 · 27 − 6 · 0.89 = −0.04 cm for women.
        (
            |c, _| c["values"]["core:height_sd_female"]["value"] = json!(27.0),
            "data.calibration_range",
        ),
        // 180.7 − 6 · 7.01 − 6 · 0.81 · 28.6 = −0.356 cm for men.
        (
            |c, _| c["values"]["core:growth_gamma"]["value"] = json!(28.6),
            "data.calibration_range",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    for key in [
        "core:height_sd_male",
        "core:height_sd_female",
        "core:growth_theta_sd_male",
        "core:growth_theta_sd_female",
        "core:growth_gamma",
    ] {
        let (mut c, g) = common::files();
        c["values"].as_object_mut().expect("values").remove(key);
        let result = Data::new(common::definitions(c, g).expect("definitions"));
        assert_eq!(result.err(), Some(DataError::CalibrationMissing), "{key}");
    }
    // The upper limit of `γ` on its own: with timing spreads of 0.01 years the corners
    // stay above 0 (180.7 − 6 · 7.01 − 6 · 0.01 · 300 = 120.64 cm), so 300 is allowed and
    // 300.5 is out of range.
    let narrow = |gamma: f64| {
        checked(move |c, _| {
            c["values"]["core:growth_theta_sd_male"]["value"] = json!(0.01);
            c["values"]["core:growth_theta_sd_female"]["value"] = json!(0.01);
            c["values"]["core:growth_gamma"]["value"] = json!(gamma);
        })
    };
    assert_eq!(narrow(300.0), "ok");
    assert!(narrow(300.5).starts_with("data.calibration_range"));
    // A corner of exactly 0 is rejected: (180 − 6 · 29) + 1 · ((θ − 6 · 1) − θ) = 0 cm.
    let zero = checked(|c, _| {
        c["values"]["core:height_adult_male"]["value"] = json!(180.0);
        c["values"]["core:height_sd_male"]["value"] = json!(29.0);
        c["values"]["core:growth_theta_sd_male"]["value"] = json!(1.0);
    });
    assert!(zero.starts_with("data.calibration_range"), "{zero}");
    // The limits are allowed, and a corner just above 0: 167.3 − 6 · 26.9 − 6 · 0.89 =
    // 0.56 cm; with 5 years 180.7 − 6 · 7.01 − 6 · 5 = 108.64 cm.
    let (mut c, g) = common::files();
    set(&mut c, "core:height_sd_female", 26.9);
    set(&mut c, "core:growth_theta_sd_male", 5.0);
    let data = Data::new(common::definitions(c, g).expect("definitions")).expect("checked");
    let female = data.genetics().growth(sim_core::genome::Sex::Female);
    assert_eq!(female.sd, 26.9);
    let corner = sim_core::growth::IndividualCurve::new(female, -6.0, -6.0);
    assert!((corner.target - 0.56).abs() < 1e-9, "{}", corner.target);
}

#[test]
fn calibration_rejects() {
    let cases: [(Change, &str); 12] = [
        (|c, _| c["format_version"] = json!(0), "data.format_version"),
        (|c, _| c["format_version"] = json!(6), "data.format_version"),
        (|c, _| c["format_version"] = json!(8), "data.format_version"),
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

/// ADR-0017 section 2, genetics specification 7.2: the Gompertz `a` and `b`, the Makeham
/// term and `σ_g` per year (logit for `σ_g`), each inside the range of the death math, so
/// that every age at death is finite; and the trait of longevity, if named, is a trait of
/// the map.
#[test]
fn death_values_reject() {
    let cases: [(Change, &str); 13] = [
        (
            |c, _| c["values"]["core:senescence_a_female"]["unit"] = json!("fraction"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:accident_male"]["unit"] = json!("ratio"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:longevity_sigma_g"]["unit"] = json!("per_year"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:senescence_a_female"]["value"] = json!(9e-16),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:senescence_a_male"]["value"] = json!(1.01),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:senescence_b_female"]["value"] = json!(9e-4),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:senescence_b_male"]["value"] = json!(1.01),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:accident_female"]["value"] = json!(9e-10),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:accident_male"]["value"] = json!(1000.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:longevity_sigma_g"]["value"] = json!(5.01),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:longevity_sigma_g"]["value"] = json!(0.0),
            "data.calibration_range",
        ),
        (
            |c, _| {
                c["values"]
                    .as_object_mut()
                    .expect("values")
                    .remove("core:senescence_b_male");
            },
            "data.calibration_missing",
        ),
        (
            |c, _| c["longevity_trait"] = json!("test:longevity"),
            "data.trait_named",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // The limits are allowed.
    let limits = checked(|c, _| {
        let values = &mut c["values"];
        values["core:senescence_a_female"]["value"] = json!(1e-15);
        values["core:senescence_a_male"]["value"] = json!(1.0);
        values["core:senescence_b_female"]["value"] = json!(1e-3);
        values["core:senescence_b_male"]["value"] = json!(1.0);
        values["core:accident_female"]["value"] = json!(1e-9);
        values["core:accident_male"]["value"] = json!(1000.0);
        values["core:longevity_sigma_g"]["value"] = json!(5.0);
    });
    assert_eq!(limits, "ok");
    // A trait of the map may be named; the data hash takes it in.
    assert_eq!(
        checked_traits(|c, _| c["longevity_trait"] = json!("test:growth")),
        "ok"
    );
    assert!(
        checked_traits(|c, _| c["longevity_trait"] = json!("test:longevity"))
            .starts_with("data.trait_named")
    );
    let (mut c, g) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    c["longevity_trait"] = json!("test:growth");
    let named = Data::new(common::definitions(c, g).expect("definitions")).expect("checked");
    assert_eq!(
        named.longevity_trait().map(DataId::as_str),
        Some("test:growth")
    );
    assert_ne!(named.hash(), 0x96ec_ba88_f9e4_f738);
    assert_eq!(common::data().longevity_trait(), None);
    // The data name the trait or say `null` explicitly (ADR-0017 section 1).
    let missing = checked(|c, _| {
        c.as_object_mut().expect("object").remove("longevity_trait");
    });
    assert!(
        missing.starts_with("serde: missing field `longevity_trait`"),
        "{missing}"
    );
}

/// The curves of fertility (genetics specification 5.6, ADR-0017 section 1): each number
/// in its unit and range, the bounds of the menarche and the ages of each decline in
/// order; the window of the test data; the trait of the puberty timing.
#[test]
fn fertility_curves_reject() {
    let curve = |field: &'static str, x: f64| {
        move |c: &mut Value, _: &mut Value| c["fertility"]["puberty"][field]["value"] = json!(x)
    };
    let mut cases: Vec<(BoxedChange, &str)> = vec![
        (
            Box::new(|c, _| c["fertility"]["puberty"]["female_ramp"]["unit"] = json!("fraction")),
            "data.calibration_unit",
        ),
        (
            Box::new(|c, _| c["fertility"]["puberty"]["male_floor"]["unit"] = json!("year")),
            "data.calibration_unit",
        ),
        (
            Box::new(|c, _| c["puberty_trait"] = json!("test:puberty")),
            "data.trait_named",
        ),
        (
            Box::new(|c, _| {
                c.as_object_mut().expect("object").remove("puberty_trait");
            }),
            "serde: missing field `puberty_trait`",
        ),
        (
            Box::new(|c, _| {
                c.as_object_mut().expect("object").remove("fertility");
            }),
            "serde: missing field `fertility`",
        ),
        (
            Box::new(|c, _| {
                c["fertility"]["puberty"]
                    .as_object_mut()
                    .expect("puberty")
                    .remove("male_lead");
            }),
            "serde: missing field `male_lead`",
        ),
    ];
    for (field, x) in [
        ("menarche_mean", 10.5),
        ("menarche_sd", 0.0),
        ("menarche_sd", 10.5),
        ("menarche_low", 3.0),
        ("menarche_high", -1.0),
        ("menarche_low", -10.5),
        ("female_ramp", 0.0),
        ("female_ramp", 20.5),
        ("female_decline_from", 0.0),
        ("female_decline_from", 50.0),
        ("female_decline_to", 130.5),
        ("male_lead", -0.1),
        ("male_lead", 10.5),
        ("male_ramp", 0.0),
        ("male_decline_from", 70.0),
        ("male_decline_to", 131.0),
        ("male_floor", -0.1),
        ("male_floor", 1.1),
    ] {
        cases.push((Box::new(curve(field, x)), "data.calibration_range"));
    }
    let window = |from: f64, until: f64| {
        move |c: &mut Value, _: &mut Value| {
            c["fertility"] = json!({"window": {
                "from": {"value": from, "unit": "year"},
                "until": {"value": until, "unit": "year"}
            }});
        }
    };
    for (from, until) in [(45.0, 18.0), (18.0, 18.0), (-1.0, 45.0), (18.0, 130.5)] {
        cases.push((Box::new(window(from, until)), "data.calibration_range"));
    }
    let count = cases.len();
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    assert_eq!(count, 28);
    // The limits, a window and a named trait of the map are allowed; the repository uses
    // the curves of 5.6 (ADR-0017 section 1).
    let limits = checked(|c, _| {
        let p = &mut c["fertility"]["puberty"];
        p["menarche_mean"]["value"] = json!(-10.0);
        p["menarche_sd"]["value"] = json!(10.0);
        p["menarche_low"]["value"] = json!(-10.0);
        p["menarche_high"]["value"] = json!(10.0);
        p["female_ramp"]["value"] = json!(20.0);
        p["female_decline_to"]["value"] = json!(130.0);
        p["male_lead"]["value"] = json!(0.0);
        p["male_floor"]["value"] = json!(0.0);
    });
    assert_eq!(limits, "ok");
    assert_eq!(checked(window(0.0, 130.0)), "ok");
    // The window is variant 1 in the data hash (`tools/data_reference.py` on the
    // repository's data with the window 18 to 45).
    let (mut c, mut g) = common::files();
    window(18.0, 45.0)(&mut c, &mut g);
    let windowed = Data::new(common::definitions(c, g).expect("definitions")).expect("checked");
    assert_eq!(windowed.hash(), 0xe89a_fdd8_dd36_d30c);
    assert_eq!(
        checked_traits(|c, _| c["puberty_trait"] = json!("test:growth")),
        "ok"
    );
    let (mut c, g) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    c["puberty_trait"] = json!("test:growth");
    let named = Data::new(common::definitions(c, g).expect("definitions")).expect("checked");
    assert_eq!(
        named.puberty_trait().map(DataId::as_str),
        Some("test:growth")
    );
    let data = common::data();
    assert!(matches!(
        data.genetics().fertility,
        sim_core::fertility::Curves::Puberty(_)
    ));
    assert_eq!(data.puberty_trait(), None);
}

/// The traits the partner market compares (11.2, 11.1): required fields, `null` in the
/// repository; a named one must be a trait of the map with phenotype weights.
#[test]
fn pairing_traits_reject() {
    let data = common::data();
    assert_eq!(data.pairing_traits(), [None, None]);
    for field in ["cognition_trait", "height_trait"] {
        let missing = checked_traits(|c, _| {
            c.as_object_mut().expect("object").remove(field);
        });
        assert_eq!(
            missing,
            format!("serde: missing field `{field}`"),
            "{field}"
        );
        // Not in the map; in the map but without phenotype weights; with them.
        let named = |id: &'static str| checked_traits(move |c, _| c[field] = json!(id));
        assert!(
            named("test:nope").starts_with("data.trait_named"),
            "{field}"
        );
        assert!(
            named("test:growth").starts_with("data.trait_named"),
            "{field}"
        );
        assert_eq!(named("test:mood"), "ok", "{field}");
    }
    let (mut c, g) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    c["cognition_trait"] = json!("test:mood");
    c["height_trait"] = json!("test:face");
    let named = Data::new(common::definitions(c, g).expect("definitions")).expect("checked");
    assert_eq!(
        named.pairing_traits().map(|t| t.map(DataId::as_str)),
        [Some("test:mood"), Some("test:face")]
    );
}

/// ADR-0013 section 3: `k` and the windows of the nutrition loss.
#[test]
fn nutrition_values_reject() {
    let cases: [(Change, &str); 10] = [
        (
            |c, _| c["values"]["core:nutrition_loss"]["unit"] = json!("centimeter"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:nutrition_weight_child"]["unit"] = json!("year"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:nutrition_age_infant"]["unit"] = json!("fraction"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:nutrition_loss"]["value"] = json!(300.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:nutrition_weight_puberty"]["value"] = json!(1.01),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:nutrition_weight_infant"]["value"] = json!(0.0),
            "data.calibration_range",
        ),
        // The first window ends before the second.
        (
            |c, _| c["values"]["core:nutrition_age_infant"]["value"] = json!(5.0),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:nutrition_age_child"]["value"] = json!(30.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:nutrition_puberty_before"]["value"] = json!(10.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:nutrition_puberty_after"]["value"] = json!(10.5),
            "data.calibration_range",
        ),
    ];
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    for key in [
        "core:nutrition_loss",
        "core:nutrition_weight_infant",
        "core:nutrition_weight_child",
        "core:nutrition_weight_puberty",
        "core:nutrition_age_infant",
        "core:nutrition_age_child",
        "core:nutrition_puberty_before",
        "core:nutrition_puberty_after",
    ] {
        let (mut c, g) = common::files();
        c["values"].as_object_mut().expect("values").remove(key);
        let result = Data::new(common::definitions(c, g).expect("definitions"));
        assert_eq!(result.err(), Some(DataError::CalibrationMissing), "{key}");
    }
    // The limits are allowed: k 300, weights 1, the windows up to 30 and 10 years, and a
    // first window just shorter than the second.
    assert_eq!(
        checked(|c, _| {
            c["values"]["core:nutrition_loss"]["value"] = json!(300.0);
            c["values"]["core:nutrition_weight_puberty"]["value"] = json!(1.0);
            c["values"]["core:nutrition_age_infant"]["value"] = json!(29.9);
            c["values"]["core:nutrition_age_child"]["value"] = json!(30.0);
            c["values"]["core:nutrition_puberty_before"]["value"] = json!(10.0);
            c["values"]["core:nutrition_puberty_after"]["value"] = json!(10.0);
        }),
        "ok"
    );
}

/// A change of the two files that captures values.
type BoxedChange = Box<dyn FnOnce(&mut Value, &mut Value)>;

/// The check of the fixture with traits (`tests/golden/data_traits`) changed by `change`.
fn checked_traits(change: impl FnOnce(&mut Value, &mut Value)) -> String {
    let (mut calibration, mut genome_map) = common::files_from(
        include_str!("../../../tests/golden/data_traits/calibration/genetics.json"),
        include_str!("../../../tests/golden/data_traits/genetics/genome_map.json"),
    );
    change(&mut calibration, &mut genome_map);
    match common::definitions(calibration, genome_map) {
        Err(serde) => format!("serde: {serde}"),
        Ok(definitions) => match Data::new(definitions) {
            Ok(_) => "ok".to_owned(),
            Err(error) => error.to_string(),
        },
    }
}

/// ADR-0014 section 1: the list of sex differences and the values of puberty and age.
#[test]
fn sex_difference_values_reject() {
    let row = |t: &str, d: f64, vr: f64| {
        json!({
            "trait": t,
            "shift": {"value": d, "unit": "standard_deviation"},
            "variance_ratio": {"value": vr, "unit": "ratio"}
        })
    };
    let cases: [(BoxedChange, &str); 9] = [
        // A trait without a phenotype row, one not in the map, two in the wrong order.
        (
            Box::new(move |c, _| c["sex_differences"] = json!([row("test:growth", 0.1, 1.0)])),
            "data.calibration_unknown",
        ),
        (
            Box::new(move |c, _| c["sex_differences"] = json!([row("test:none", 0.1, 1.0)])),
            "data.calibration_unknown",
        ),
        (
            Box::new(move |c, _| {
                c["sex_differences"] =
                    json!([row("test:mood", 0.1, 1.0), row("test:face", 0.1, 1.0)]);
            }),
            "data.calibration_unknown",
        ),
        (
            Box::new(|c, _| c["sex_differences"][0]["shift"]["unit"] = json!("ratio")),
            "data.calibration_unit",
        ),
        (
            Box::new(|c, _| {
                c["sex_differences"][0]["variance_ratio"]["unit"] = json!("standard_deviation");
            }),
            "data.calibration_unit",
        ),
        (
            Box::new(|c, _| c["sex_differences"][0]["shift"]["value"] = json!(-10.5)),
            "data.calibration_range",
        ),
        (
            Box::new(|c, _| c["sex_differences"][0]["variance_ratio"]["value"] = json!(0.0)),
            "data.calibration_range",
        ),
        (
            Box::new(|c, _| c["sex_differences"][0]["variance_ratio"]["value"] = json!(10.5)),
            "data.calibration_range",
        ),
        (
            Box::new(|c, _| {
                c["sex_differences"]
                    .as_array_mut()
                    .expect("list")
                    .push(json!({"trait": "test:mood", "shift": 1}));
            }),
            "serde",
        ),
    ];
    for (change, code) in cases {
        let result = checked_traits(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // The ends are allowed: a shift of ±10 and 0, a variance ratio of 10.
    for (d, vr) in [(-10.0, 10.0), (10.0, 0.01), (0.0, 1.0)] {
        assert_eq!(
            checked_traits(move |c, _| {
                c["sex_differences"][0]["shift"]["value"] = json!(d);
                c["sex_differences"][0]["variance_ratio"]["value"] = json!(vr);
            }),
            "ok",
            "{d} {vr}"
        );
    }
    // The values of puberty and age.
    let values: [(Change, &str); 6] = [
        (
            |c, _| c["values"]["core:puberty_half_width"]["value"] = json!(10.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:height_loss_onset"]["value"] = json!(130.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:height_loss_male"]["value"] = json!(1.5),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:height_loss_female"]["value"] = json!(0.0),
            "data.calibration_range",
        ),
        (
            |c, _| c["values"]["core:height_loss_male"]["unit"] = json!("centimeter_per_year"),
            "data.calibration_unit",
        ),
        (
            |c, _| c["values"]["core:puberty_half_width"]["unit"] = json!("ratio"),
            "data.calibration_unit",
        ),
    ];
    for (change, code) in values {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    for key in [
        "core:puberty_half_width",
        "core:height_loss_onset",
        "core:height_loss_male",
        "core:height_loss_female",
    ] {
        let (mut c, g) = common::files();
        c["values"].as_object_mut().expect("values").remove(key);
        let result = Data::new(common::definitions(c, g).expect("definitions"));
        assert_eq!(result.err(), Some(DataError::CalibrationMissing), "{key}");
    }
    // A calibration without the list is no calibration (ADR-0014: format 3).
    assert!(
        checked(|c, _| {
            c.as_object_mut().expect("file").remove("sex_differences");
        })
        .starts_with("serde")
    );
}

/// ADR-0015 section 1: every number of specification section 10 is a calibration value with
/// its unit, and the weight of 7.4.
#[test]
fn body_values_are_those_of_the_specification() {
    let expected: [(&str, f64, &str); 71] = [
        ("core:body_pigment_skin_offset", -1.2, "logit"),
        ("core:body_pigment_skin_slope", 1.15, "logit"),
        ("core:body_pigment_hair_slope", 1.2, "logit"),
        ("core:body_pigment_hair_residual", 0.55, "logit"),
        ("core:body_pigment_hair_darkening", 1.0, "logit"),
        ("core:body_pigment_hair_darkening_age", 12.0, "year"),
        ("core:body_pigment_iris_offset", 0.5, "logit"),
        ("core:body_pigment_iris_slope", 1.5, "logit"),
        ("core:body_pigment_iris_darkening", 1.4, "logit"),
        ("core:body_pigment_iris_darkening_age", 3.0, "year"),
        ("core:body_pigment_skin_pheo_offset", -1.5, "logit"),
        ("core:body_pigment_skin_pheo_slope", -0.25, "logit"),
        ("core:body_pigment_hair_pheo_offset", -2.0, "logit"),
        ("core:body_pigment_hair_pheo_slope", -0.35, "logit"),
        ("core:body_mc1r_hair_eu", 0.3, "fraction"),
        ("core:body_mc1r_skin_eu", 0.55, "fraction"),
        ("core:body_mc1r_hair_pheo", 0.8, "fraction"),
        ("core:body_mc1r_skin_pheo", 0.45, "fraction"),
        ("core:body_mc1r_freckle", 1.0, "logit"),
        ("core:body_herc2_iris", 0.15, "fraction"),
        ("core:body_development_androgen_lag", 0.5, "year"),
        ("core:body_development_estrogen_lead", 0.5, "year"),
        ("core:body_development_breast_lead", 0.2, "year"),
        ("core:body_development_tempo", 1.15, "year"),
        ("core:body_development_androgen_offset", 1.5, "logit"),
        ("core:body_development_estrogen_offset", 1.5, "logit"),
        ("core:body_development_estrogen_men", 0.1, "fraction"),
        ("core:body_development_androgen_women", 0.15, "fraction"),
        ("core:body_curl_offset", -1.4, "logit"),
        ("core:body_curl_slope", 1.5, "logit"),
        ("core:body_freckle_offset", -1.6, "logit"),
        ("core:body_freckle_slope", 1.1, "logit"),
        ("core:body_freckle_pigment", -0.5, "logit"),
        ("core:body_beard_lag", 4.0, "year"),
        ("core:body_beard_tempo", 1.8, "year"),
        ("core:body_beard_offset", 0.4, "logit"),
        ("core:body_beard_androgen", 0.4, "logit"),
        ("core:body_body_hair_lag", 2.5, "year"),
        ("core:body_body_hair_tempo", 1.6, "year"),
        ("core:body_body_hair_androgen", 0.4, "logit"),
        ("core:body_body_hair_women", 0.2, "fraction"),
        ("core:body_bald_men_stage", 7.0, "stage"),
        ("core:body_bald_men_center", 64.0, "year"),
        ("core:body_bald_men_slope", 8.0, "year"),
        ("core:body_bald_men_androgen", 3.0, "year"),
        ("core:body_bald_men_tempo", 10.0, "year"),
        ("core:body_bald_women_stage", 2.5, "stage"),
        ("core:body_bald_women_center", 78.0, "year"),
        ("core:body_bald_women_slope", 6.0, "year"),
        ("core:body_bald_women_tempo", 12.0, "year"),
        ("core:body_hair_density_offset", 2.0, "logit"),
        ("core:body_hair_density_loss", 0.3, "fraction"),
        ("core:body_hair_density_center", 60.0, "year"),
        ("core:body_hair_density_tempo", 8.0, "year"),
        ("core:body_grey_center", 55.0, "year"),
        ("core:body_grey_slope", 7.0, "year"),
        ("core:body_grey_tempo", 7.0, "year"),
        ("core:body_wrinkles_center", 55.0, "year"),
        ("core:body_wrinkles_slope", 5.0, "year"),
        ("core:body_wrinkles_tempo", 8.0, "year"),
        ("core:body_laxity_center", 62.0, "year"),
        ("core:body_laxity_slope", 5.0, "year"),
        ("core:body_laxity_tempo", 8.0, "year"),
        ("core:body_spots_center", 60.0, "year"),
        ("core:body_spots_skin", 4.0, "year"),
        ("core:body_spots_pigment", 2.0, "year"),
        ("core:body_spots_tempo", 8.0, "year"),
        ("core:body_stoop_center", 78.0, "year"),
        ("core:body_stoop_skin", 4.0, "year"),
        ("core:body_stoop_tempo", 6.0, "year"),
        ("core:bio_age_weight", 0.5, "fraction"),
    ];
    let (calibration, _) = common::files();
    for (key, value, unit) in expected {
        let entry = &calibration["values"][key];
        assert_eq!(entry["value"].as_f64(), Some(value), "{key}");
        assert_eq!(entry["unit"].as_str(), Some(unit), "{key}");
    }
    let model = common::data().genetics().body;
    assert_eq!(model.pigment_skin_offset, -1.2);
    assert_eq!(model.stoop_tempo, 6.0);
    assert_eq!(common::data().genetics().bio_age_weight, 0.5);
}

/// Genetics specification 2.3, 7.5 and 11.3 (W11.3d), ADR-0017 section 4: the sex ratio
/// at birth is the calibration value `core:sex_ratio_at_birth`, 1.05 sons per daughter, unit
/// `ratio`; above 0 and at most 10, the upper end allowed, a value beyond, another unit
/// and a missing value rejected.
#[test]
fn the_sex_ratio_at_birth_is_a_calibration_value() {
    const KEY: &str = "core:sex_ratio_at_birth";
    let (calibration, _) = common::files();
    assert_eq!(calibration["values"][KEY]["value"].as_f64(), Some(1.05));
    assert_eq!(calibration["values"][KEY]["unit"].as_str(), Some("ratio"));
    for (value, result) in [
        (10.0, "ok"),
        (1e-9, "ok"),
        (10.5, "data.calibration_range"),
        (0.0, "data.calibration_range"),
        (-1.05, "data.calibration_range"),
    ] {
        let got = checked(|c, _| c["values"][KEY]["value"] = json!(value));
        assert!(got.starts_with(result), "{value}: {got}");
    }
    let got = checked(|c, _| c["values"][KEY]["unit"] = json!("fraction"));
    assert!(got.starts_with("data.calibration_unit"), "{got}");
    let (mut c, g) = common::files();
    c["values"].as_object_mut().expect("values").remove(KEY);
    let result = Data::new(common::definitions(c, g).expect("definitions"));
    assert_eq!(result.err(), Some(DataError::CalibrationMissing));
}

/// ADR-0015 section 1: `logit` from −20 to 20 (0 and negatives allowed), `stage` above 0 up
/// to 7; each end allowed, a value beyond rejected; a missing value rejected.
#[test]
fn body_values_reject() {
    for (key, value, result) in [
        ("core:body_pigment_skin_offset", -20.0, "ok"),
        ("core:body_pigment_skin_offset", 20.0, "ok"),
        ("core:body_pigment_skin_offset", 0.0, "ok"),
        (
            "core:body_pigment_skin_offset",
            -20.5,
            "data.calibration_range",
        ),
        ("core:body_curl_slope", 20.5, "data.calibration_range"),
        ("core:body_bald_men_stage", 7.0, "ok"),
        ("core:body_bald_men_stage", 7.5, "data.calibration_range"),
        ("core:body_bald_women_stage", 0.0, "data.calibration_range"),
        ("core:body_grey_center", 130.0, "ok"),
        ("core:body_grey_center", 130.5, "data.calibration_range"),
        ("core:body_beard_tempo", 0.0, "data.calibration_range"),
        ("core:body_mc1r_hair_eu", 1.0, "ok"),
        ("core:body_mc1r_hair_eu", 1.01, "data.calibration_range"),
        ("core:bio_age_weight", 0.0, "data.calibration_range"),
    ] {
        let got = checked(|c, _| c["values"][key]["value"] = json!(value));
        assert!(got.starts_with(result), "{key} {value}: {got}");
    }
    for (key, unit) in [
        ("core:body_pigment_skin_offset", "year"),
        ("core:body_bald_men_stage", "fraction"),
        ("core:body_grey_center", "logit"),
    ] {
        let got = checked(|c, _| c["values"][key]["unit"] = json!(unit));
        assert!(got.starts_with("data.calibration_unit"), "{key}: {got}");
    }
    let (mut c, g) = common::files();
    c["values"]
        .as_object_mut()
        .expect("values")
        .remove("core:body_stoop_tempo");
    let result = Data::new(common::definitions(c, g).expect("definitions"));
    assert_eq!(result.err(), Some(DataError::CalibrationMissing));
}

/// ADR-0015 section 1: every key reaches its own field of `BodyModel`. Each field holds
/// the value of the table; then every value moves by its own step (`0.001 · (i + 1)`
/// down, inside every range), and each field follows its key.
#[test]
fn every_body_value_reaches_its_field() {
    type Field = fn(&sim_core::data::BodyModel) -> f64;
    let fields: [(&str, Field, f64); 70] = [
        (
            "core:body_pigment_skin_offset",
            |m| m.pigment_skin_offset,
            -1.2,
        ),
        (
            "core:body_pigment_skin_slope",
            |m| m.pigment_skin_slope,
            1.15,
        ),
        (
            "core:body_pigment_hair_slope",
            |m| m.pigment_hair_slope,
            1.2,
        ),
        (
            "core:body_pigment_hair_residual",
            |m| m.pigment_hair_residual,
            0.55,
        ),
        (
            "core:body_pigment_hair_darkening",
            |m| m.pigment_hair_darkening,
            1.0,
        ),
        (
            "core:body_pigment_hair_darkening_age",
            |m| m.pigment_hair_darkening_age,
            12.0,
        ),
        (
            "core:body_pigment_iris_offset",
            |m| m.pigment_iris_offset,
            0.5,
        ),
        (
            "core:body_pigment_iris_slope",
            |m| m.pigment_iris_slope,
            1.5,
        ),
        (
            "core:body_pigment_iris_darkening",
            |m| m.pigment_iris_darkening,
            1.4,
        ),
        (
            "core:body_pigment_iris_darkening_age",
            |m| m.pigment_iris_darkening_age,
            3.0,
        ),
        (
            "core:body_pigment_skin_pheo_offset",
            |m| m.pigment_skin_pheo_offset,
            -1.5,
        ),
        (
            "core:body_pigment_skin_pheo_slope",
            |m| m.pigment_skin_pheo_slope,
            -0.25,
        ),
        (
            "core:body_pigment_hair_pheo_offset",
            |m| m.pigment_hair_pheo_offset,
            -2.0,
        ),
        (
            "core:body_pigment_hair_pheo_slope",
            |m| m.pigment_hair_pheo_slope,
            -0.35,
        ),
        ("core:body_mc1r_hair_eu", |m| m.mc1r_hair_eu, 0.3),
        ("core:body_mc1r_skin_eu", |m| m.mc1r_skin_eu, 0.55),
        ("core:body_mc1r_hair_pheo", |m| m.mc1r_hair_pheo, 0.8),
        ("core:body_mc1r_skin_pheo", |m| m.mc1r_skin_pheo, 0.45),
        ("core:body_mc1r_freckle", |m| m.mc1r_freckle, 1.0),
        ("core:body_herc2_iris", |m| m.herc2_iris, 0.15),
        (
            "core:body_development_androgen_lag",
            |m| m.development_androgen_lag,
            0.5,
        ),
        (
            "core:body_development_estrogen_lead",
            |m| m.development_estrogen_lead,
            0.5,
        ),
        (
            "core:body_development_breast_lead",
            |m| m.development_breast_lead,
            0.2,
        ),
        ("core:body_development_tempo", |m| m.development_tempo, 1.15),
        (
            "core:body_development_androgen_offset",
            |m| m.development_androgen_offset,
            1.5,
        ),
        (
            "core:body_development_estrogen_offset",
            |m| m.development_estrogen_offset,
            1.5,
        ),
        (
            "core:body_development_estrogen_men",
            |m| m.development_estrogen_men,
            0.1,
        ),
        (
            "core:body_development_androgen_women",
            |m| m.development_androgen_women,
            0.15,
        ),
        ("core:body_curl_offset", |m| m.curl_offset, -1.4),
        ("core:body_curl_slope", |m| m.curl_slope, 1.5),
        ("core:body_freckle_offset", |m| m.freckle_offset, -1.6),
        ("core:body_freckle_slope", |m| m.freckle_slope, 1.1),
        ("core:body_freckle_pigment", |m| m.freckle_pigment, -0.5),
        ("core:body_beard_lag", |m| m.beard_lag, 4.0),
        ("core:body_beard_tempo", |m| m.beard_tempo, 1.8),
        ("core:body_beard_offset", |m| m.beard_offset, 0.4),
        ("core:body_beard_androgen", |m| m.beard_androgen, 0.4),
        ("core:body_body_hair_lag", |m| m.body_hair_lag, 2.5),
        ("core:body_body_hair_tempo", |m| m.body_hair_tempo, 1.6),
        (
            "core:body_body_hair_androgen",
            |m| m.body_hair_androgen,
            0.4,
        ),
        ("core:body_body_hair_women", |m| m.body_hair_women, 0.2),
        ("core:body_bald_men_stage", |m| m.bald_men_stage, 7.0),
        ("core:body_bald_men_center", |m| m.bald_men_center, 64.0),
        ("core:body_bald_men_slope", |m| m.bald_men_slope, 8.0),
        ("core:body_bald_men_androgen", |m| m.bald_men_androgen, 3.0),
        ("core:body_bald_men_tempo", |m| m.bald_men_tempo, 10.0),
        ("core:body_bald_women_stage", |m| m.bald_women_stage, 2.5),
        ("core:body_bald_women_center", |m| m.bald_women_center, 78.0),
        ("core:body_bald_women_slope", |m| m.bald_women_slope, 6.0),
        ("core:body_bald_women_tempo", |m| m.bald_women_tempo, 12.0),
        (
            "core:body_hair_density_offset",
            |m| m.hair_density_offset,
            2.0,
        ),
        ("core:body_hair_density_loss", |m| m.hair_density_loss, 0.3),
        (
            "core:body_hair_density_center",
            |m| m.hair_density_center,
            60.0,
        ),
        (
            "core:body_hair_density_tempo",
            |m| m.hair_density_tempo,
            8.0,
        ),
        ("core:body_grey_center", |m| m.grey_center, 55.0),
        ("core:body_grey_slope", |m| m.grey_slope, 7.0),
        ("core:body_grey_tempo", |m| m.grey_tempo, 7.0),
        ("core:body_wrinkles_center", |m| m.wrinkles_center, 55.0),
        ("core:body_wrinkles_slope", |m| m.wrinkles_slope, 5.0),
        ("core:body_wrinkles_tempo", |m| m.wrinkles_tempo, 8.0),
        ("core:body_laxity_center", |m| m.laxity_center, 62.0),
        ("core:body_laxity_slope", |m| m.laxity_slope, 5.0),
        ("core:body_laxity_tempo", |m| m.laxity_tempo, 8.0),
        ("core:body_spots_center", |m| m.spots_center, 60.0),
        ("core:body_spots_skin", |m| m.spots_skin, 4.0),
        ("core:body_spots_pigment", |m| m.spots_pigment, 2.0),
        ("core:body_spots_tempo", |m| m.spots_tempo, 8.0),
        ("core:body_stoop_center", |m| m.stoop_center, 78.0),
        ("core:body_stoop_skin", |m| m.stoop_skin, 4.0),
        ("core:body_stoop_tempo", |m| m.stoop_tempo, 6.0),
    ];
    let model = common::data().genetics().body;
    for (key, get, value) in fields {
        assert_eq!(get(&model), value, "{key}");
    }
    let (mut c, g) = common::files();
    let mut moved = Vec::new();
    for (step, (key, _, value)) in (1..).zip(fields) {
        let new = value - 0.001 * f64::from(step);
        c["values"][key]["value"] = json!(new);
        moved.push(new);
    }
    let data = Data::new(common::definitions(c, g).expect("definitions")).expect("checked");
    let model = data.genetics().body;
    for ((key, get, _), new) in fields.into_iter().zip(moved) {
        assert_eq!(get(&model), new, "{key}");
    }
}

/// The map of the repository plus the trait `test:a` with one additive locus `test:s` on
/// the free slot 0 of segment 1, next to the loci of the diseases.
fn with_liability(g: &mut Value) {
    g["traits"] = json!([trait_("test:a", 1.0)]);
    let mut loci = g["loci"].as_array().expect("loci").clone();
    loci.insert(
        1,
        json!({
            "id": "test:s",
            "chromosome": "autosome",
            "segment": 1,
            "slot": 0,
            "action": {"additive": {"effect": 0.5, "d_ref": 1.0, "loads": [on("test:a", 1.0)]}},
            "mark": "small",
            "mutation": "both"
        }),
    );
    g["loci"] = json!(loci);
}

/// The number of a calibration value inside a disease.
fn number(c: &Calibrated) -> f64 {
    c.value.get()
}

/// The six archetypes of genetics specification 3.2 in the order of the data, which is the
/// order of their causes of death (ADR-0017 sections 1 and 2): carrier, penetrance, onset
/// and effect. The two steps are those of 3.2: `h0 = H_18 / 18` (W3.2j, W3.2m) and for
/// cystic fibrosis `h1 = (ln 2 − 0.02) / 47` (W3.2k, from Python), so `H(65) = ln 2`.
#[test]
fn the_diseases_are_those_of_the_specification() {
    let data = common::data();
    let diseases = data.diseases();
    let codes: Vec<&str> = diseases.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes,
        [
            "core:disease_ar",
            "core:disease_ad_late",
            "core:disease_ad_partial",
            "core:disease_xr",
            "core:disease_mt",
            "core:disease_liability"
        ]
    );
    let locus = |i: usize| match &diseases[i].carrier {
        Carrier::Locus {
            locus,
            penetrance_female,
            penetrance_male,
        } => (
            locus.as_str().to_owned(),
            number(penetrance_female),
            number(penetrance_male),
        ),
        Carrier::Threshold { .. } => panic!("a locus"),
    };
    assert_eq!(locus(0), ("core:disease_ar".to_owned(), 1.0, 1.0));
    assert_eq!(locus(1), ("core:disease_ad_late".to_owned(), 1.0, 1.0));
    assert_eq!(locus(2), ("core:disease_ad_partial".to_owned(), 0.7, 0.1));
    assert_eq!(locus(3), ("core:disease_xr".to_owned(), 1.0, 1.0));
    assert_eq!(locus(4), ("core:disease_mt".to_owned(), 0.1, 0.5));
    match &diseases[5].carrier {
        Carrier::Threshold {
            liability,
            heritability,
            prevalence,
        } => {
            assert_eq!(
                *liability, None,
                "no trait of liability before the pre-run (11.1)"
            );
            assert_eq!((number(heritability), number(prevalence)), (0.6, 0.01));
        }
        Carrier::Locus { .. } => panic!("the threshold"),
    }
    let onset = |i: usize| match &diseases[i].onset {
        Onset::Fixed { age } => vec![number(age)],
        Onset::Normal {
            mean,
            sd,
            low,
            high,
        } => {
            vec![number(mean), number(sd), number(low), number(high)]
        }
    };
    assert_eq!(onset(0), [0.0]);
    assert_eq!(onset(1), [45.0, 10.0, 20.0, 80.0]);
    assert_eq!(onset(2), [50.0, 10.0, 25.0, 85.0]);
    assert_eq!(onset(3), [0.0]);
    assert_eq!(onset(4), [25.0, 8.0, 10.0, 60.0]);
    assert_eq!(onset(5), [25.0, 5.0, 15.0, 45.0]);
    let effect = |i: usize| match &diseases[i].effect {
        Effect::Death { per_year } => vec![number(per_year)],
        Effect::DeathInTwoSteps {
            before,
            change_age,
            after,
        } => vec![number(before), number(change_age), number(after)],
        Effect::Visible => Vec::new(),
    };
    assert_eq!(effect(0), [0.02 / 18.0, 18.0, 0.014_322_280_437_445_644]);
    assert_eq!(effect(1), [0.04]);
    assert_eq!(effect(2), [0.02]);
    assert_eq!(effect(3), [0.01 / 18.0, 18.0, 0.005]);
    assert_eq!(effect(4), Vec::<f64>::new());
    assert_eq!(effect(5), [0.01]);
}

/// ADR-0017 section 1: the checks of the diseases, `data.disease` for a value or a
/// carrier that does not fit, a trait of liability the map lacks among them (ADR-0020
/// section 2; the other rules of a named liability in `tests/threshold.rs`).
#[test]
fn diseases_reject() {
    let cases: [(Change, &str); 29] = [
        (|c, _| c["format_version"] = json!(3), "data.format_version"),
        // The carrier: a locus of the map, marked disease, recessive or dominant, one
        // disease per locus; codes unique.
        (
            |c, _| c["diseases"][0]["carrier"]["locus"]["locus"] = json!("core:nothing"),
            "data.disease",
        ),
        (
            |c, g| {
                with_liability(g);
                c["diseases"][0]["carrier"]["locus"]["locus"] = json!("test:s");
            },
            "data.disease",
        ),
        (
            |c, _| c["diseases"][1]["carrier"]["locus"]["locus"] = json!("core:disease_ar"),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][1]["code"] = json!("core:disease_ar"),
            "data.disease",
        ),
        // The causes of death for everyone keep their names (ADR-0017 section 2).
        (
            |c, _| c["diseases"][1]["code"] = json!("core:accident"),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][5]["code"] = json!("core:senescence"),
            "data.disease",
        ),
        // Penetrance, a fraction.
        (
            |c, _| c["diseases"][2]["carrier"]["locus"]["penetrance_female"]["value"] = json!(1.1),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][2]["carrier"]["locus"]["penetrance_male"]["value"] = json!(-0.1),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][2]["carrier"]["locus"]["penetrance_male"]["unit"] = json!("year"),
            "data.calibration_unit",
        ),
        // Onset: ages from 0 to 130, a positive spread, bounds in order.
        (
            |c, _| c["diseases"][1]["onset"]["normal"]["low"]["value"] = json!(81.0),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][1]["onset"]["normal"]["sd"]["value"] = json!(0.0),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][1]["onset"]["normal"]["high"]["value"] = json!(131.0),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][1]["onset"]["normal"]["mean"]["value"] = json!(-1.0),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][0]["onset"]["fixed"]["age"]["value"] = json!(-1.0),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][0]["onset"]["fixed"]["age"]["unit"] = json!("per_year"),
            "data.calibration_unit",
        ),
        // Effect: the ranges of the death math (`mortality`), a death has a hazard.
        (
            |c, _| c["diseases"][1]["effect"]["death"]["per_year"]["value"] = json!(0.0),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][1]["effect"]["death"]["per_year"]["value"] = json!(1001.0),
            "data.disease",
        ),
        (
            |c, _| {
                let steps = &mut c["diseases"][0]["effect"]["death_in_two_steps"];
                steps["before"]["value"] = json!(0.0);
                steps["after"]["value"] = json!(0.0);
            },
            "data.disease",
        ),
        (
            |c, _| {
                c["diseases"][0]["effect"]["death_in_two_steps"]["before"]["value"] = json!(1e-10);
            },
            "data.disease",
        ),
        (
            |c, _| {
                c["diseases"][0]["effect"]["death_in_two_steps"]["change_age"]["value"] =
                    json!(0.0);
            },
            "data.disease",
        ),
        (
            |c, _| {
                c["diseases"][0]["effect"]["death_in_two_steps"]["change_age"]["value"] =
                    json!(131.0);
            },
            "data.disease",
        ),
        // The threshold: a trait of the map, a heritability and a prevalence in (0, 1)
        // and (0, 0.5].
        (
            |c, _| c["diseases"][5]["carrier"]["threshold"]["liability"] = json!("test:a"),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][5]["carrier"]["threshold"]["heritability"]["value"] = json!(1.0),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][5]["carrier"]["threshold"]["heritability"]["value"] = json!(0.0),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][5]["carrier"]["threshold"]["prevalence"]["value"] = json!(0.0),
            "data.disease",
        ),
        // A mendelian locus that is not marked disease (mutants: the mark alone decides).
        (|_, g| g["loci"][0]["mark"] = json!("large"), "data.disease"),
        (
            |c, _| c["diseases"][5]["carrier"]["threshold"]["prevalence"]["value"] = json!(0.6),
            "data.disease",
        ),
        (
            |c, _| c["diseases"][4]["effect"] = json!("explodes"),
            "serde",
        ),
    ];
    for (change, code) in cases {
        let result = checked_diseases(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    // A trait of liability the map has, but not named after its disease (ADR-0020 section 2).
    assert_eq!(
        checked_diseases(|c, g| {
            with_liability(g);
            c["diseases"][5]["carrier"]["threshold"]["liability"] = json!("test:a");
        }),
        "data.disease"
    );
    // Valid variants: no diseases, a penetrance of 0.
    assert_eq!(checked_diseases(|c, _| c["diseases"] = json!([])), "ok");
    assert_eq!(
        checked_diseases(|c, _| {
            c["diseases"][2]["carrier"]["locus"]["penetrance_male"]["value"] = json!(0.0);
        }),
        "ok"
    );
    // Two steps with one of them 0: the old edge (nobody dies before the change), or a
    // risk only before it (mutants: either step alone is enough).
    for (before, after) in [(0.0, 0.01), (0.01, 0.0)] {
        assert_eq!(
            checked_diseases(|c, _| {
                let steps = &mut c["diseases"][0]["effect"]["death_in_two_steps"];
                steps["before"]["value"] = json!(before);
                steps["after"]["value"] = json!(after);
            }),
            "ok",
            "{before} {after}"
        );
    }
    assert_eq!(
        checked_diseases(|c, _| {
            let diseases = c["diseases"].as_array_mut().expect("diseases");
            diseases.push(json!({"code": "test:x"}));
        }),
        "serde: missing field `carrier`"
    );
}

/// ADR-0023 section 3 and its addendum: the shock templates are checked (an id twice, no
/// year, an index outside [0, 1) each with its code), they enter the data hash after the
/// definitions in the order of their ids, and data without templates keep their hash. A
/// template one ulp apart gives another hash.
#[test]
fn the_shock_templates_are_checked_and_hashed() {
    let hunger = |id: &str, years: u32, index: f64| ShockTemplate {
        id: DataId::new(id).expect("valid"),
        kind: ShockKind::Hunger,
        years,
        index: sim_core::finite::FiniteF64::try_new(index).expect("finite"),
    };
    let base = common::data();
    let none = base.clone().with_shocks(Vec::new()).expect("no templates");
    assert_eq!(none.hash(), base.hash());
    for (templates, code) in [
        (
            vec![hunger("core:a", 1, 0.7), hunger("core:a", 3, 0.7)],
            DataError::ShocksDuplicate,
        ),
        (vec![hunger("core:a", 0, 0.7)], DataError::ShocksYears),
        (vec![hunger("core:a", 1, 1.0)], DataError::ShocksIndex),
        (vec![hunger("core:a", 1, -0.1)], DataError::ShocksIndex),
    ] {
        assert_eq!(base.clone().with_shocks(templates).err(), Some(code));
    }
    let both = |first, second| {
        base.clone()
            .with_shocks(vec![first, second])
            .expect("checked")
    };
    let ordered = both(hunger("core:a", 1, 0.7), hunger("core:b", 3, 0.7));
    let swapped = both(hunger("core:b", 3, 0.7), hunger("core:a", 1, 0.7));
    assert_eq!(ordered.hash(), swapped.hash(), "in the order of their ids");
    assert_ne!(ordered.hash(), base.hash());
    let next = f64::from_bits(0.7_f64.to_bits() + 1);
    let ulp = both(hunger("core:a", 1, next), hunger("core:b", 3, 0.7));
    assert_ne!(ulp.hash(), ordered.hash());
}
