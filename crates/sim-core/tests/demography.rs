//! The life course data (ADR-0017 section 1): `data/calibration/demography.json`, read and
//! checked by `Data::new`. Expected values from the genetics specification 11.2, 11.3 and
//! 11.5, ADR-0018 section 3 and the fit of `tools/life_reference.py` (2026-10-10).

mod common;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::demography::{Fertility, Market, Twins};
use sim_core::learning::LearningParams;

/// The check of the repository's definitions with the life course data changed by
/// `change`: "ok" or the code of the error (or the message of serde).
fn checked(change: impl FnOnce(&mut Value)) -> String {
    let mut demography = common::demography();
    change(&mut demography);
    let (calibration, genome_map) = common::files();
    match common::definitions_with(demography, calibration, genome_map) {
        Err(serde) => format!("serde: {serde}"),
        Ok(definitions) => match Data::new(definitions) {
            Ok(_) => "ok".to_owned(),
            Err(error) => error.to_string(),
        },
    }
}

/// A change of the life course data.
type Change = Box<dyn FnOnce(&mut Value)>;

/// Sets the value of `key`.
fn set(demography: &mut Value, key: &str, value: f64) {
    demography["values"][key]["value"] = json!(value);
}

#[test]
fn the_repository_values_are_those_of_the_specification() {
    let data = common::data();
    let demography = data.demography();
    assert_eq!(
        demography.fertility,
        Fertility {
            adult_rate: 0.2,
            pause_months: 12,
            pregnancy_months: 10,
            wish_tilt: 0.311,
            wish_shape: vec![0.10, 0.15, 0.40, 0.25, 0.10],
        }
    );
    assert_eq!(
        demography.twins,
        Twins {
            identical: 0.004,
            fraternal: 0.01,
        }
    );
    assert_eq!(
        demography.market,
        Market {
            age_years: 16,
            search_height: 0.01179,
            search_center: 25.23,
            search_width: 7.0,
            minor_factor: 0.0918,
            candidates: 12,
            weight_cognition: 0.377,
            weight_height: 0.164,
            weight_age: 0.02,
            age_gap: 4.69,
            kinship_limit: 0.125,
            cousin_odds: 10.0,
        }
    );
    assert_eq!(demography.separation_rate, 0.00867);
    assert_eq!(
        demography.learning,
        LearningParams {
            kappa: 0.25,
            nu: 0.2,
            tau0: 1000.0,
        }
    );
}

/// One case per check of the life course data: the format, a value missing, unknown or in
/// another unit, and every range just outside.
#[test]
fn demography_rejects() {
    let value = |key: &'static str, x: f64| move |d: &mut Value| set(d, key, x);
    let mut cases: Vec<(Change, &str)> = vec![
        (
            Box::new(|d| d["format_version"] = json!(0)),
            "data.format_version",
        ),
        (
            Box::new(|d| d["format_version"] = json!(3)),
            "data.format_version",
        ),
        (
            Box::new(|d| {
                d["values"]
                    .as_object_mut()
                    .expect("values")
                    .remove("core:separation_rate");
            }),
            "data.calibration_missing",
        ),
        (
            Box::new(|d| {
                d["values"]["core:extra"] = json!({"value": 1.0, "unit": "fraction"});
            }),
            "data.calibration_unknown",
        ),
        (
            Box::new(|d| d["values"]["core:fertility_pause"]["unit"] = json!("year")),
            "data.calibration_unit",
        ),
        (
            Box::new(|d| d["values"]["core:learning_time"]["unit"] = json!("year")),
            "data.calibration_unit",
        ),
        (
            Box::new(|d| d["fertility_wish_shape"][2]["unit"] = json!("ratio")),
            "data.calibration_unit",
        ),
        (
            Box::new(|d| {
                d.as_object_mut()
                    .expect("object")
                    .remove("fertility_wish_shape");
            }),
            "serde: missing field `fertility_wish_shape`",
        ),
        (
            Box::new(|d| d["note"] = json!("x")),
            "serde: unknown field `note`",
        ),
    ];
    // Every range just outside (design note of the life course data, section 4).
    for (key, x) in [
        ("core:fertility_adult_rate", 0.0),
        ("core:fertility_adult_rate", 1.01),
        ("core:fertility_pause", -1.0),
        ("core:fertility_pause", 12.5),
        ("core:fertility_pause", 131.0),
        ("core:pregnancy_length", 0.0),
        ("core:pregnancy_length", 9.5),
        ("core:pregnancy_length", 21.0),
        ("core:fertility_wish_tilt", 0.0),
        ("core:fertility_wish_tilt", 10.5),
        ("core:twins_identical", -0.001),
        ("core:twins_identical", 1.01),
        ("core:twins_fraternal", 1.01),
        ("core:market_age", 15.0),
        ("core:market_age", 16.5),
        ("core:market_age", 121.0),
        ("core:market_search_height_female", -0.01),
        ("core:market_search_height_female", 1.01),
        ("core:market_search_center_female", 0.0),
        ("core:market_search_center_female", 150.5),
        ("core:market_search_width", 0.0),
        ("core:market_search_width", 151.0),
        ("core:market_search_factor_minor", 0.0),
        ("core:market_search_factor_minor", 1.01),
        ("core:market_candidates", 0.0),
        ("core:market_candidates", 12.5),
        ("core:market_candidates", 1001.0),
        ("core:pairing_weight_cognition", 0.0),
        ("core:pairing_weight_height", 1.1e6),
        ("core:pairing_weight_age", 0.0),
        ("core:pairing_age_gap_preferred", 0.0),
        ("core:pairing_age_gap_preferred", 151.0),
        ("core:pairing_kinship_limit", 0.0),
        ("core:pairing_kinship_limit", 1.01),
        ("core:pairing_cousin_odds", 0.99),
        ("core:pairing_cousin_odds", 1.1e6),
        ("core:separation_rate", -0.001),
        ("core:separation_rate", 5e-10),
        ("core:separation_rate", 1000.5),
        ("core:learning_plateau", 0.0),
        ("core:learning_plateau", 10.5),
        ("core:learning_speed", 0.0),
        ("core:learning_time", 0.0),
        ("core:learning_time", 1.1e9),
    ] {
        cases.push((Box::new(value(key, x)), "data.demography"));
    }
    // Twins together above 1.
    cases.push((
        Box::new(|d| {
            set(d, "core:twins_identical", 0.6);
            set(d, "core:twins_fraternal", 0.5);
        }),
        "data.demography",
    ));
    // The wish shape: empty, too long, all zero, a weight outside [0, 1].
    let weight = |x: f64| json!({"value": x, "unit": "fraction"});
    for shape in [
        json!([]),
        Value::Array(vec![weight(0.5); 65]),
        json!([weight(0.0), weight(0.0)]),
        json!([weight(0.5), weight(1.01)]),
        json!([weight(0.5), weight(-0.1)]),
    ] {
        cases.push((
            Box::new(move |d| d["fertility_wish_shape"] = shape),
            "data.demography",
        ));
    }
    let count = cases.len();
    for (change, code) in cases {
        let result = checked(change);
        assert!(result.starts_with(code), "{code}: {result}");
    }
    assert_eq!(count, 59);
}

/// The limits themselves are allowed, and zero where ADR-0017 section 1 allows it: twins,
/// a weight of the wish, the pause, the height of the search rate, separation.
#[test]
fn demography_limits_are_allowed() {
    let lower = checked(|d| {
        set(d, "core:fertility_pause", 0.0);
        set(d, "core:pregnancy_length", 1.0);
        set(d, "core:twins_identical", 0.0);
        set(d, "core:twins_fraternal", 0.0);
        set(d, "core:market_age", 16.0);
        set(d, "core:market_search_height_female", 0.0);
        set(d, "core:market_candidates", 1.0);
        set(d, "core:pairing_cousin_odds", 1.0);
        set(d, "core:separation_rate", 0.0);
        d["fertility_wish_shape"] = json!([{"value": 1.0, "unit": "fraction"}]);
    });
    assert_eq!(lower, "ok");
    let upper = checked(|d| {
        set(d, "core:fertility_adult_rate", 1.0);
        set(d, "core:fertility_pause", 130.0);
        set(d, "core:pregnancy_length", 20.0);
        set(d, "core:fertility_wish_tilt", 10.0);
        set(d, "core:twins_identical", 0.5);
        set(d, "core:twins_fraternal", 0.5);
        set(d, "core:market_age", 120.0);
        set(d, "core:market_search_height_female", 1.0);
        set(d, "core:market_search_center_female", 150.0);
        set(d, "core:market_search_width", 150.0);
        set(d, "core:market_search_factor_minor", 1.0);
        set(d, "core:market_candidates", 1000.0);
        set(d, "core:pairing_weight_cognition", 1e6);
        set(d, "core:pairing_weight_height", 1e6);
        set(d, "core:pairing_weight_age", 1e6);
        set(d, "core:pairing_age_gap_preferred", 150.0);
        set(d, "core:pairing_kinship_limit", 1.0);
        set(d, "core:pairing_cousin_odds", 1e6);
        set(d, "core:separation_rate", 1000.0);
        set(d, "core:learning_plateau", 10.0);
        set(d, "core:learning_speed", 10.0);
        set(d, "core:learning_time", 1e9);
        let mut shape = vec![json!({"value": 0.0, "unit": "fraction"}); 63];
        shape.push(json!({"value": 1.0, "unit": "fraction"}));
        d["fertility_wish_shape"] = Value::Array(shape);
    });
    assert_eq!(upper, "ok");
    // The smallest separation rate above zero is the smallest hazard.
    assert_eq!(checked(|d| set(d, "core:separation_rate", 1e-9)), "ok");
}

/// The life course data are part of the data hash (ADR-0007 section 3), and a world
/// without separation is data, not code.
#[test]
fn the_life_course_data_count_in_the_hash() {
    let mut demography = common::demography();
    set(&mut demography, "core:separation_rate", 0.0);
    let (calibration, genome_map) = common::files();
    let without = Data::new(
        common::definitions_with(demography, calibration, genome_map).expect("definitions"),
    )
    .expect("checked");
    assert_eq!(without.demography().separation_rate, 0.0);
    assert_ne!(without.hash(), common::data().hash());
}
