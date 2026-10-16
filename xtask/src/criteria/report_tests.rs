//! The criteria report against the reports of `tools/criteria_reference.py`, computed from
//! the reference's own exports (its NOTES section 2: both sides read the same `z` bits),
//! and the report logic the three fixtures do not reach, with values derived by hand from
//! the design note.

use super::*;
use crate::criteria::stats::mc1r_expectation;
use crate::criteria::units::AGES;
use serde_json::json;

const HAND: &str = include_str!("../../../tests/golden/criteria/hand.ndjson");
const HAND_REPORT: &str = include_str!("../../../tests/golden/criteria/hand.report.json");
const T1: &str = include_str!("../../../tests/golden/criteria/t1_pool_d.ndjson");
const T1_REPORT: &str = include_str!("../../../tests/golden/criteria/t1_pool_d.report.json");
const T2: &str = include_str!("../../../tests/golden/criteria/t2_pool_c.ndjson");
const T2_REPORT: &str = include_str!("../../../tests/golden/criteria/t2_pool_c.report.json");

/// A year in seconds, as the life table's exposure counts it.
const YS: u64 = 31_449_600;

/// `x` within `1e-12 · max(1, |x|)` of `want` (design note section 2).
fn close(x: f64, want: f64) -> bool {
    let scale = if want.abs() > 1.0 { want.abs() } else { 1.0 };
    (x - want).abs() <= 1e-12 * scale
}

fn float(value: &Value) -> Option<f64> {
    let text = value.as_str()?;
    (text.len() == 16)
        .then(|| u64::from_str_radix(text, 16).ok().map(f64::from_bits))
        .flatten()
}

/// The differences of a report from the reference's: the same keys everywhere, integers,
/// strings, booleans and nulls exact, float bits exact unless the enclosing check lists the
/// field in `inexact` (`twin.value` for its twin's), then `close`.
fn differences(got: &Value, want: &Value) -> Vec<String> {
    let mut found = Vec::new();
    walk("", "", got, want, &[], &mut found);
    found
}

fn walk(
    at: &str,
    field: &str,
    got: &Value,
    want: &Value,
    inexact: &[String],
    found: &mut Vec<String>,
) {
    match (got, want) {
        (Value::Object(got), Value::Object(want)) => {
            let (have, need): (Vec<&String>, Vec<&String>) =
                (got.keys().collect(), want.keys().collect());
            if have != need {
                found.push(format!("{at}: keys {have:?}, want {need:?}"));
                return;
            }
            let listed: Option<Vec<String>> =
                want.get("inexact").and_then(Value::as_array).map(|names| {
                    names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                });
            let (inexact, field) = match &listed {
                Some(names) => (names.as_slice(), ""),
                None => (inexact, field),
            };
            for (key, value) in want {
                let name = if field.is_empty() {
                    key.clone()
                } else {
                    format!("{field}.{key}")
                };
                walk(
                    &format!("{at}/{key}"),
                    &name,
                    &got[key],
                    value,
                    inexact,
                    found,
                );
            }
        }
        (Value::Array(got), Value::Array(want)) if got.len() == want.len() => {
            for (i, (got, want)) in got.iter().zip(want).enumerate() {
                walk(&format!("{at}/{i}"), field, got, want, inexact, found);
            }
        }
        _ => {
            let loose = inexact.iter().any(|name| name == field);
            let agree = match (loose, float(got), float(want)) {
                (true, Some(x), Some(w)) => close(x, w),
                _ => got == want,
            };
            if !agree {
                found.push(format!("{at}: {got}, want {want}"));
            }
        }
    }
}

fn against_reference(export: &str, want: &str) {
    let got = report(export).expect("a report");
    let want: Value = serde_json::from_str(want).expect("the reference's report");
    let found = differences(&got, &want);
    assert!(
        found.is_empty(),
        "{} differences:\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn the_hand_report_is_the_references() {
    against_reference(HAND, HAND_REPORT);
}

#[test]
fn the_report_of_t1_is_the_references() {
    against_reference(T1, T1_REPORT);
}

#[test]
fn the_report_of_t2_is_the_references() {
    against_reference(T2, T2_REPORT);
}

#[test]
fn the_comparison_holds_keys_and_exact_bits_and_loosens_only_listed_fields() {
    let want = json!({"checks": [{
        "check": "x", "n": 1, "value": "3ff0000000000000", "r": "3ff0000000000000",
        "inexact": ["value"],
        "twin": {"value": "3ff0000000000000", "r": "3ff0000000000000"},
    }]});
    assert!(differences(&want, &want).is_empty());
    let with = |path: &str, value: Value| {
        let mut got = want.clone();
        *got.pointer_mut(path).expect("a path") = value;
        differences(&got, &want)
    };
    // One ulp in a listed field agrees; 2e-12 does not, nor one ulp in an exact field.
    assert!(with("/checks/0/value", json!("3ff0000000000001")).is_empty());
    let off = format!("{:016x}", (1.0_f64 + 2e-12).to_bits());
    assert_eq!(with("/checks/0/value", json!(off)).len(), 1);
    assert_eq!(with("/checks/0/r", json!("3ff0000000000001")).len(), 1);
    assert_eq!(
        with("/checks/0/twin/value", json!("3ff0000000000001")).len(),
        1
    );
    assert_eq!(with("/checks/0/n", json!(2)).len(), 1);
    let mut listed = want.clone();
    listed["checks"][0]["inexact"] = json!(["value", "twin.value"]);
    let mut got = listed.clone();
    got["checks"][0]["twin"]["value"] = json!("3ff0000000000001");
    assert!(differences(&got, &listed).is_empty());
    let mut extra = want.clone();
    extra["checks"][0]["twin"]["k"] = json!(0);
    assert_eq!(differences(&extra, &want).len(), 1, "an extra key");
    assert_eq!(differences(&want, &extra).len(), 1, "a missing key");
}

fn verdict(pass: Option<bool>, finding: Option<&'static str>, fragile: bool) -> Verdict {
    Verdict {
        pass,
        finding,
        fragile,
    }
}

#[test]
fn a_verdict_reads_the_minimum_then_the_values_then_the_tolerance() {
    let red = |finding| verdict(Some(false), Some(finding), false);
    assert_eq!(
        against(5, 10, Some(0.0), 0.0, Some(1.0)),
        red("below_minimum")
    );
    assert_eq!(against(10, 10, None, 0.0, Some(1.0)), red("undefined"));
    assert_eq!(against(10, 10, Some(0.0), 0.0, None), red("undefined"));
    assert_eq!(against(10, 10, Some(1.0), 0.8, Some(0.1)), red("outside"));
    assert_eq!(
        against(0, 0, Some(-0.25), 0.0, Some(0.5)),
        verdict(Some(true), None, false)
    );
    // `|value − target| ≤ tol` holds its boundary, which is fragile (within 1e-9), on
    // either side.
    assert_eq!(
        against(10, 10, Some(1.5), 1.0, Some(0.5)),
        verdict(Some(true), None, true)
    );
    assert_eq!(
        against(10, 10, Some(1.5 + 5e-10), 1.0, Some(0.5)),
        verdict(Some(false), Some("outside"), true)
    );
    // `fragile` is the reference's rule whatever `n`: below the minimum too (review).
    assert_eq!(
        against(1, 10, Some(1.5), 1.0, Some(0.5)),
        verdict(Some(false), Some("below_minimum"), true)
    );
    // A NaN value or tolerance is outside, never green and never fragile (review).
    for (value, tol) in [(f64::NAN, 0.5), (1.0, f64::NAN)] {
        assert_eq!(against(10, 10, Some(value), 1.0, Some(tol)), red("outside"));
    }
    // Reported only: no verdict, `fragile` as for a check.
    assert_eq!(
        Verdict::reported(Some(1.5), 1.0, Some(0.5)),
        verdict(None, None, true)
    );
    assert_eq!(
        Verdict::reported(Some(1.2), 1.0, Some(0.5)),
        verdict(None, None, false)
    );
}

#[test]
fn a_band_holds_its_ends() {
    let band = [2.5, 4.0];
    assert_eq!(within(Some(3.0), band), verdict(Some(true), None, false));
    // The ends belong to the band; a band is never fragile (the reference's rule, review).
    assert_eq!(within(Some(2.5), band), verdict(Some(true), None, false));
    assert_eq!(within(Some(4.0), band), verdict(Some(true), None, false));
    assert_eq!(
        within(Some(f64::NAN), band),
        verdict(Some(false), Some("outside"), false)
    );
    assert_eq!(
        within(Some(4.5), band),
        verdict(Some(false), Some("outside"), false)
    );
    assert_eq!(
        within(Some(2.0), band),
        verdict(Some(false), Some("outside"), false)
    );
    assert_eq!(
        within(None, band),
        verdict(Some(false), Some("undefined"), false)
    );
}

fn hex(x: f64) -> Value {
    json!(format!("{:016x}", x.to_bits()))
}

/// A life table of a year's exposure at every age with one death in the open interval.
fn table() -> LifeInput {
    let mut deaths = [0; AGES];
    deaths[100] = 1;
    LifeInput {
        exposure: [YS; AGES],
        deaths,
        anchors: vec![3, 1],
    }
}

#[test]
fn the_life_table_check_reads_e0_and_its_error() {
    // No death before 100: l = 1 throughout, T_0 = 100 + l_ω / m_ω = 101; the variance is
    // the open term l_ω² D_ω / (y_ω² m_ω⁴) = 1.
    let check = life("f11_e0_f", 83.43, &table());
    assert_eq!(check["n"], json!(2));
    assert_eq!(
        check["units"],
        json!(format!("{:016x}", unit_hash(&[3, 1])))
    );
    assert_eq!(check["value"], hex(101.0));
    assert_eq!(check["se"], hex(1.0));
    assert_eq!(check["tol"], hex(4.0));
    assert_eq!(check["target"], hex(83.43));
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(false), &json!("outside"))
    );
    assert_eq!(check["zero_exposure"], json!([]));
    assert_eq!(check["exposure"][100], json!(YS));
    assert_eq!(check["inexact"], json!([]));
    // One death at 80: q_80 = 1 / 1.5 = 2/3, l = 1/3 from 81; T_0 = 80 + 2/3 + 19/3 + 1/3
    // = 262/3; e_81 = 20, Var = 20.5² (4/9)(1/3) + (1/9) = 1684/27.
    let mut one = table();
    one.deaths[80] = 1;
    let check = life("f11_e0_f", 83.43, &one);
    let (e0, se) = (
        float(&check["value"]).expect("e_0"),
        float(&check["se"]).expect("se"),
    );
    assert!(close(e0, 262.0 / 3.0), "{e0}");
    assert!(close(se, sim_math::sqrt(1684.0 / 27.0)), "{se}");
    assert_eq!(check["tol"], hex(4.0 * se));
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(true), &Value::Null)
    );
}

#[test]
fn the_life_table_check_names_each_finding() {
    let finding = |input: &LifeInput| {
        let check = life("f11_e0_m", 78.8, input);
        assert_eq!(
            (&check["value"], &check["se"], &check["tol"], &check["pass"]),
            (&Value::Null, &Value::Null, &Value::Null, &json!(false))
        );
        (check["finding"].clone(), check["zero_exposure"].clone())
    };
    let mut gaps = table();
    gaps.exposure[5] = 0;
    gaps.exposure[7] = 0;
    assert_eq!(finding(&gaps), (json!("zero_exposure"), json!([5, 7])));
    let mut no_open = table();
    no_open.deaths[100] = 0;
    assert_eq!(finding(&no_open), (json!("no_open_deaths"), json!([])));
    // Two deaths in a year at 80: m = 2, q = 2 / 2 = 1.
    let mut q_one = table();
    q_one.deaths[80] = 2;
    assert_eq!(finding(&q_one), (json!("q_at_least_one"), json!([])));
    // Two deaths in a year and a second at every age: q just below 1, l under 1e-323 by 42.
    let mut under = table();
    under.exposure[..100].fill(YS + 1);
    under.deaths[..100].fill(2);
    assert_eq!(finding(&under), (json!("l_underflow"), json!([])));
}

#[test]
fn births_of_one_woman_have_a_mean_and_no_error() {
    let check = births(&Units {
        values: vec![2],
        anchors: vec![7],
    });
    assert_eq!(check["value"], hex(2.0));
    assert_eq!(
        (&check["variance"], &check["se"], &check["tol"]),
        (&Value::Null, &Value::Null, &Value::Null)
    );
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(false), &json!("undefined"))
    );
    assert_eq!(check["total"], json!(2));
    // Two women with 1 and 3: mean 2, V = (1 + 1) / 1 = 2, se = sqrt(2 / 2) = 1.
    let check = births(&Units {
        values: vec![1, 3],
        anchors: vec![7, 8],
    });
    assert_eq!(
        [
            &check["value"],
            &check["variance"],
            &check["se"],
            &check["tol"]
        ],
        [&hex(2.0), &hex(2.0), &hex(1.0), &hex(4.0)]
    );
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(true), &Value::Null)
    );
    assert_eq!((&check["total"], &check["n"]), (&json!(4), &json!(2)));
}

#[test]
fn the_bands_are_undefined_without_units_and_pass_inside() {
    let check = early_births(&Units::default());
    assert_eq!((&check["n"], &check["early"]), (&json!(0), &json!(0)));
    assert_eq!(check["value"], Value::Null);
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(false), &json!("undefined"))
    );
    assert_eq!(check["band"], json!([hex(0.001), hex(0.005)]));
    // One early birth in 400: 0.0025.
    let check = early_births(&Units {
        values: (0..400).map(|i| i == 9).collect(),
        anchors: (0..400).collect(),
    });
    assert_eq!(check["value"], hex(0.0025));
    assert_eq!((&check["pass"], &check["early"]), (&json!(true), &json!(1)));
    let check = age_gap(&Units {
        values: vec![3.0, 3.5],
        anchors: vec![1, 2, 3, 4],
    });
    assert_eq!(check["value"], hex(3.25));
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(true), &Value::Null)
    );
    assert_eq!(
        (&check["target"], &check["tol"]),
        (&Value::Null, &Value::Null)
    );
    let check = age_gap(&Units::default());
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(false), &json!("undefined"))
    );
}

#[test]
fn a_decade_without_variance_is_undefined_and_one_at_its_minimum_passes() {
    let flat = decade(
        3,
        &Units {
            values: vec![0.0; 500],
            anchors: (0..500).collect(),
        },
    );
    assert_eq!(
        (&flat["n"], &flat["k"], &flat["check"]),
        (&json!(500), &json!(3), &json!("p4_city_3"))
    );
    assert_eq!(flat["variance"], hex(0.0));
    assert_eq!(flat["value"], Value::Null);
    assert_eq!(
        (&flat["pass"], &flat["finding"]),
        (&json!(false), &json!("undefined"))
    );
    assert_eq!(flat["twin"]["finding"], json!("undefined"));
    // ±1 in turn: mean 0, V = 500 / 499, ln V = 0.002 inside 4 sqrt(2 / 499) = 0.2532; the
    // twin's V is 0.664 times as large, ln 0.664 V = -0.407 outside.
    let check = decade(
        0,
        &Units {
            values: (0..500)
                .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
                .collect(),
            anchors: (0..500).collect(),
        },
    );
    assert_eq!(check["variance"], hex(500.0 / 499.0));
    assert_eq!(check["tol"], hex(4.0 * sim_math::sqrt(2.0 / 499.0)));
    let value = float(&check["value"]).expect("ln V");
    assert!(close(value, sim_math::ln(500.0 / 499.0)), "{value}");
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(true), &Value::Null)
    );
    let twin = float(&check["twin"]["value"]).expect("the twin's ln V");
    assert!(
        (twin - sim_math::ln(0.664 * 500.0 / 499.0)).abs() < 1e-9,
        "{twin}"
    );
    assert_eq!(
        (&check["twin"]["pass"], &check["twin"]["finding"]),
        (&json!(false), &json!("outside"))
    );
    // W8.4d at the minimum.
    let power = float(&check["power"]).expect("a power");
    assert!((power - 0.9932).abs() < 5e-5, "{power}");
}

#[test]
fn mc1r_at_its_minimum_passes_and_its_twin_over_every_family_keeps_the_count() {
    // 400 families, each its own sample family with one zygote (E_1 = p² = 0.7225), 289 of
    // them of unaffected parents: the share equals the expectation. q = 120 / 800 = 0.15.
    let sample = Mc1r {
        copies: 120,
        parents: 400,
        q: Some(0.15),
        units: Units {
            values: (0..400).map(|i| (i < 289, mc1r_expectation(1))).collect(),
            anchors: (0..400).collect(),
        },
        families: (0..400).collect(),
        unaffected: (0..400).map(|i| i < 289).collect(),
    };
    let check = mc1r(&sample);
    assert_eq!(
        (&check["n"], &check["k"], &check["affected"]),
        (&json!(400), &json!(289), &json!(400))
    );
    assert_eq!(
        (&check["q_copies"], &check["q_parents"]),
        (&json!(120), &json!(400))
    );
    assert_eq!(check["value"], hex(0.7225));
    let target = float(&check["target"]).expect("a target");
    assert!(close(target, 0.7225), "{target}");
    // se = sqrt(400 · 0.7225 · 0.2775) / 400.
    let tol = float(&check["tol"]).expect("a tolerance");
    assert!(
        close(tol, 4.0 * sim_math::sqrt(400.0 * 0.7225 * 0.2775) / 400.0),
        "{tol}"
    );
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(true), &Value::Null)
    );
    assert!(float(&check["power"]).expect("a power") > 0.9999);
    // The permutation runs over all 400 families, all of them sampled: the twin draws
    // every family's parents once, 289 unaffected again.
    assert_eq!(
        (&check["twin"]["k"], &check["twin"]["value"]),
        (&json!(289), &hex(0.7225))
    );
    assert_eq!(check["twin"]["pass"], json!(true));
}

#[test]
fn mc1r_without_an_affected_family_is_below_its_minimum() {
    let check = mc1r(&Mc1r {
        copies: 3,
        parents: 10,
        q: Some(0.15),
        units: Units::default(),
        families: Vec::new(),
        unaffected: vec![true; 5],
    });
    assert_eq!((&check["n"], &check["k"]), (&json!(0), &json!(0)));
    assert_eq!(
        [
            &check["value"],
            &check["target"],
            &check["tol"],
            &check["power"]
        ],
        [&Value::Null; 4]
    );
    assert_eq!(
        (&check["pass"], &check["finding"]),
        (&json!(false), &json!("below_minimum"))
    );
    assert_eq!(check["twin"]["finding"], json!("below_minimum"));
}

fn village_of(values: Vec<f64>, living: u64) -> Village {
    let anchors = (42..).take(values.len()).collect();
    Village {
        block: 0,
        units: Units { values, anchors },
        blocks: vec![(0, 2, 2)],
        living,
    }
}

#[test]
fn the_village_block_reports_its_findings_with_its_numbers() {
    let none = village(14, None);
    assert_eq!(
        none,
        json!({
            "check": "village_block", "end_years": 14, "g_star": null, "n": 0,
            "units": format!("{:016x}", unit_hash(&[])), "variance": null,
            "expectation": null, "blocks": [], "living": null, "finding": "no_block",
        })
    );
    // N_e = 4 · 2 · 2 / 4 = 4: E = 0.8 (1 − 1/8) + 0.2 = 0.9.
    let one = village(60, Some(&village_of(vec![1.0], 3)));
    assert_eq!(
        (&one["g_star"], &one["n"], &one["living"]),
        (&json!(0), &json!(1), &json!(3))
    );
    assert_eq!(one["units"], json!(format!("{:016x}", unit_hash(&[42]))));
    assert_eq!(
        (&one["variance"], &one["finding"]),
        (&Value::Null, &json!("fewer_than_two_units"))
    );
    assert!(close(float(&one["expectation"]).expect("E"), 0.9));
    assert_eq!(one["blocks"], json!([[0, 2, 2]]));
    let dead = village(60, Some(&village_of(vec![1.0, 3.0], 0)));
    assert_eq!(
        (&dead["variance"], &dead["finding"]),
        (&hex(2.0), &json!("no_living_person"))
    );
    let alive = village(60, Some(&village_of(vec![1.0, 3.0], 5)));
    assert_eq!(alive["finding"], Value::Null);
    // A block without parent blocks has no expectation, not the empty product 1 (review).
    let bare = Village {
        blocks: Vec::new(),
        ..village_of(vec![1.0, 3.0], 5)
    };
    assert_eq!(village(60, Some(&bare))["expectation"], Value::Null);
}

#[test]
fn the_report_goes_beside_the_export_unless_named() {
    let export = std::path::Path::new("runs/a/export.ndjson");
    assert_eq!(
        destination(export, None),
        std::path::Path::new("runs/a/report.json")
    );
    assert_eq!(
        destination(export, Some("elsewhere.json")),
        std::path::Path::new("elsewhere.json")
    );
    assert!(command(&[]).is_err());
    assert!(command(&["town".to_owned()]).is_err());
}
