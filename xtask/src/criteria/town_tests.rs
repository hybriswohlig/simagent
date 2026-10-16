//! The criteria town's plumbing on the reference's two small pool towns (design note of the
//! criteria kit, section 7, fixture 3): run, export and report in Rust give the reference's
//! verdicts, check by check; and what makes a report red.

use serde_json::{Value, json};
use sim_core::time::SimTime;

use super::*;

const POOL_C: &[u8] = include_bytes!("../../../tests/golden/pool/pool_c.bin");
const POOL_D: &[u8] = include_bytes!("../../../tests/golden/pool/pool_d.bin");
const T1_REPORT: &str = include_str!("../../../tests/golden/criteria/t1_pool_d.report.json");
const T2_REPORT: &str = include_str!("../../../tests/golden/criteria/t2_pool_c.report.json");

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace")
        .to_path_buf()
}

/// What decides each check: its name, size, units, verdict and finding, and its twin's.
fn decisions(report: &Value) -> Vec<Value> {
    let decide = |check: &Value| {
        json!([
            check["check"],
            check["n"],
            check["units"],
            check["pass"],
            check["finding"],
            check["twin"]["pass"],
            check["twin"]["finding"],
            check["twin_pass"],
        ])
    };
    let mut all = Vec::new();
    for check in report["checks"].as_array().expect("checks") {
        all.push(decide(check));
        for decade in check["decades"].as_array().into_iter().flatten() {
            all.push(decide(decade));
        }
    }
    all.push(report["village_block"].clone());
    all
}

/// The whole pipeline in Rust (data, run, export, report) decides every check of T1 and T2
/// as the reference's report does.
#[test]
fn the_small_towns_are_decided_as_the_reference_decides() {
    let towns = [
        (POOL_D, 6, 30, false, 60, T1_REPORT),
        (POOL_C, 0xff, 50, true, 14, T2_REPORT),
    ];
    for (pool, seed, n_start, immigration, years, want) in towns {
        let data = town_data(&root(), &root().join("tests/golden/map4_template"), pool)
            .expect("the town's data");
        let plan = Plan {
            seed,
            tag: "test:arrival",
            n_start,
            capacity: None,
            immigration,
            end: SimTime::from_secs(years * YEAR - 1),
        };
        let (world, observer) = run_town(data, &plan).expect("a run");
        let text = write(&world, &observer, plan.end).expect("an export");
        let got = report(&text).expect("a report");
        let want: Value = serde_json::from_str(want).expect("the reference's report");
        assert_eq!(decisions(&got), decisions(&want), "seed {seed}");
        // Small towns are red: below the minimum, MC1R's q outside its band.
        assert!(!reds(&got).is_empty());
    }
}

/// The criteria town is the one ADR-0020 section 8 names: 10 000 from the pool, capacity
/// 10 000, immigration, 200 years, its gate seed.
#[test]
fn the_criteria_town_is_the_adrs() {
    assert_eq!(
        criteria_town(),
        Plan {
            seed: 0x5eed_0004_0000_0001,
            tag: "core:criteria_town",
            n_start: 10_000,
            capacity: Some(10_000),
            immigration: true,
            end: SimTime::from_secs(200 * 31_449_600 - 1),
        }
    );
}

#[test]
fn a_red_check_and_a_green_twin_make_a_report_red() {
    let report = json!({"checks": [
        {"check": "p3", "pass": true, "twin": {"pass": false}},
        {"check": "p5_siblings", "pass": false, "finding": "outside", "twin": {"pass": false}},
        {"check": "p9_polygenic", "pass": true, "twin": {"pass": true}},
        {"check": "p4_city", "pass": true, "twin_pass": true},
        {"check": "f11_median_m", "pass": null, "finding": null},
    ]});
    assert_eq!(
        reds(&report),
        vec![
            "p5_siblings: outside",
            "p9_polygenic: the negative twin is green",
            "p4_city: the negative twin is green",
        ]
    );
    let green = json!({"checks": [{"check": "p3", "pass": true, "twin": {"pass": false}}]});
    assert!(reds(&green).is_empty());
}

#[test]
fn the_command_names_its_three_paths() {
    let args = |list: &[&str]| list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    for wrong in [
        args(&["--map", "m", "--pool", "p"]),
        args(&["--map", "m", "--pool", "p", "--out"]),
        args(&["--map", "m", "--pool", "p", "--into", "o"]),
    ] {
        assert!(command(&root(), &wrong).is_err(), "{wrong:?}");
    }
}
