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
        assert!(!reds(&got, &|_| true).is_empty());
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
        reds(&report, &|_| true),
        vec![
            "p5_siblings: outside",
            "p9_polygenic: the negative twin is green",
            "p4_city: the negative twin is green",
        ]
    );
    let green = json!({"checks": [{"check": "p3", "pass": true, "twin": {"pass": false}}]});
    assert!(reds(&green, &|_| true).is_empty());
}

#[test]
fn the_command_names_its_three_paths() {
    let args = |list: &[&str]| list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    for wrong in [
        args(&["--map", "m", "--pool", "p"]),
        args(&["--map", "m", "--pool", "p", "--out"]),
        args(&["--map", "m", "--pool", "p", "--into", "o"]),
    ] {
        assert_eq!(command(&root(), &wrong), Err(USAGE.to_owned()), "{wrong:?}");
    }
    // Every path is read after its own name, in any order: the pool is read first, so the
    // error names exactly the pool's path (mutants).
    for right in [
        args(&["--map", "m", "--pool", "no/such/pool.bin", "--out", "o"]),
        args(&["--out", "o", "--pool", "no/such/pool.bin", "--map", "m"]),
    ] {
        let error = command(&root(), &right).expect_err("no pool");
        assert!(error.starts_with("no/such/pool.bin: "), "{error}");
    }
}

/// A frozen map's calibration still holds `pool_frequencies` (the pre-run's file); the
/// town's data drop them, as the pool was extracted without them: the same data as the
/// calibration without the entry (review).
#[test]
fn the_towns_data_drop_the_pool_frequencies() {
    let template = root().join("tests/golden/map4_template");
    let plain = town_data(&root(), &template, POOL_D).expect("the town's data");
    let dir = std::env::temp_dir().join(format!("simagent-town-{}", std::process::id()));
    let frequencies: Value = serde_json::from_str(include_str!(
        "../../../tests/golden/pool/pool_frequencies.json"
    ))
    .expect("json");
    let mut calibration: Value = serde_json::from_str(include_str!(
        "../../../tests/golden/map4_template/calibration/genetics.json"
    ))
    .expect("json");
    calibration["pool_frequencies"] = frequencies["pool_frequencies"].clone();
    for (path, text) in [
        (
            "calibration/genetics.json",
            serde_json::to_string_pretty(&calibration).expect("json"),
        ),
        (
            "genetics/genome_map.json",
            include_str!("../../../tests/golden/map4_template/genetics/genome_map.json").to_owned(),
        ),
    ] {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        std::fs::write(&file, text).expect("written");
    }
    let stripped = town_data(&root(), &dir, POOL_D);
    std::fs::remove_dir_all(&dir).expect("removed");
    assert_eq!(stripped.expect("the town's data").hash(), plain.hash());
}

/// Grok GH-1: the open town is the criteria town without capacity, and it alone decides
/// `h²_T`; the criteria town decides every other check.
#[test]
fn the_open_town_decides_h2t_and_only_it() {
    assert_eq!(
        open_town(),
        Plan {
            capacity: None,
            ..criteria_town()
        }
    );
    let report = json!({"checks": [
        {"check": "f11_h2t_f", "pass": false, "finding": "outside"},
        {"check": "f11_h2t_m", "pass": true},
        {"check": "f11_e0_m", "pass": false, "finding": "outside"},
        {"check": "p3", "pass": true, "twin": {"pass": true}},
    ]});
    assert_eq!(reds(&report, &decided_open), vec!["f11_h2t_f: outside"]);
    assert_eq!(
        reds(&report, &decided_closed),
        vec!["f11_e0_m: outside", "p3: the negative twin is green"]
    );
}
