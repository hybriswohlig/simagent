//! The criteria town's plumbing on the reference's two small pool towns (design note of the
//! criteria kit, section 7, fixture 3): run, export and report in Rust give the reference's
//! verdicts, check by check; and what makes a report red.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sim_core::finite::FiniteF64;
use sim_core::time::SimTime;

use super::*;

const POOL_B: &[u8] = include_bytes!("../../../tests/golden/pool/pool_b.bin");
const POOL_C: &[u8] = include_bytes!("../../../tests/golden/pool/pool_c.bin");
const POOL_D: &[u8] = include_bytes!("../../../tests/golden/pool/pool_d.bin");
const FREQUENCIES: &str = include_str!("../../../tests/golden/pool/pool_frequencies.json");
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

/// The frequencies this golden pool was made with, as `Data::with_pool` takes them (slice
/// S2a of the map-4 switch): those of its pool run `frequent_alleles` for `pool_d`
/// (`tools/pool_reference.py`), the file's (`tests/golden/pool/pool_frequencies.json`) for
/// every other pool.
fn frequencies(pool: &[u8]) -> BTreeMap<DataId, FiniteF64> {
    let file: Value = serde_json::from_str(FREQUENCIES).expect("json");
    let mut values: BTreeMap<String, f64> = file["pool_frequencies"]
        .as_object()
        .expect("the entries")
        .iter()
        .map(|(id, entry)| (id.clone(), entry["value"].as_f64().expect("a value")))
        .collect();
    if pool == POOL_D {
        values = [
            ("core:disease_ar", 0.4),
            ("core:disease_ad_late", 0.3),
            ("core:disease_ad_partial", 0.35),
            ("core:disease_xr", 0.45),
            ("core:disease_mt", 0.25),
            ("core:mc1r", 0.6),
            ("core:herc2", 0.55),
        ]
        .into_iter()
        .map(|(id, q)| (id.to_owned(), q))
        .collect();
    }
    values
        .into_iter()
        .map(|(id, q)| {
            let q = FiniteF64::try_new(q).expect("finite");
            (DataId::new(&id).expect("id"), q)
        })
        .collect()
}

/// The template of map 4 as a frozen map in the directory `dir`: its calibration with a
/// `pool_frequencies` entry of the frequencies `pool` was made with (the file's texts), as
/// the pre-run leaves it.
fn frozen_template(dir: &Path, pool: &[u8]) {
    let file: Value = serde_json::from_str(FREQUENCIES).expect("json");
    let mut entry = file["pool_frequencies"].clone();
    for (id, q) in frequencies(pool) {
        entry[id.as_str()]["value"] = json!(q.get());
    }
    let mut calibration: Value = serde_json::from_str(include_str!(
        "../../../tests/golden/map4_template/calibration/genetics.json"
    ))
    .expect("json");
    calibration["pool_frequencies"] = entry;
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
}

/// The town's data of `pool` on the template frozen with its frequencies.
fn frozen_town_data(label: &str, pool: &[u8]) -> Result<Data> {
    let dir = std::env::temp_dir().join(format!("simagent-{label}-{}", std::process::id()));
    frozen_template(&dir, pool);
    let data = town_data(&root(), &dir, pool);
    std::fs::remove_dir_all(&dir).expect("removed");
    data
}

/// A pool on the documents it was extracted from, whose data hash is its `frozen_hash`:
/// `pool_c` on those of `father_dies` (with their own life course data), every other pool on
/// the template with the repository's (design note of the pool's records, section 6); with
/// the frequencies it was made with.
fn small_town_data(pool: &[u8]) -> sim_core::data::Data {
    let (demography, documents) = if pool == POOL_C {
        (
            "tests/golden/father_dies/calibration/demography.json",
            "tests/golden/father_dies",
        )
    } else {
        (
            "data/calibration/demography.json",
            "tests/golden/map4_template",
        )
    };
    let demography = crate::map4::read(&root(), demography).expect("demography");
    let (calibration, map) =
        crate::prerun::read_files(&root().join(documents)).expect("the documents");
    let definitions =
        crate::prerun::definitions_of(&demography, &calibration, &map).expect("definitions");
    sim_core::data::Data::with_pool(definitions, pool, frequencies(pool)).expect("checked")
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
        let data = small_town_data(pool);
        let plan = Plan {
            seed,
            tag: "test:arrival",
            n_start,
            capacity: None,
            immigration,
            village: false,
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
            village: false,
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

/// A frozen map's calibration holds `pool_frequencies` (the pre-run's file); the town's
/// data take the pool's frequencies from it, and they enter the data hash with the pool
/// (slice S2a of the map-4 switch): the template frozen with the frequencies of a pool's run
/// gives the reference's data hash (`pool_d` with those of `frequent_alleles`, `pool_b`
/// with the file's). A calibration without the entry is an error: the pool does not load
/// without its frequencies.
#[test]
fn the_towns_data_take_the_pool_frequencies() {
    let template = root().join("tests/golden/map4_template");
    let error = town_data(&root(), &template, POOL_D).expect_err("no frequencies");
    assert!(error.starts_with("pool.frequencies_loci"), "{error}");
    for (label, pool, data_hash) in [
        ("town-d", POOL_D, 0xeccf_f256_9ac1_2bb4),
        ("town-b", POOL_B, 0x6bc8_a5d6_8de3_6a5e),
    ] {
        let data = frozen_town_data(label, pool).expect("the town's data");
        assert_eq!(data.hash(), data_hash, "{label}");
        assert_eq!(
            data.pool().expect("a pool").frequencies(),
            &frequencies(pool)
        );
    }
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

/// T1's plan (the reference's first small town).
fn t1() -> Plan {
    Plan {
        seed: 6,
        tag: "test:arrival",
        n_start: 30,
        capacity: None,
        immigration: false,
        village: false,
        end: SimTime::from_secs(60 * YEAR - 1),
    }
}

/// Nothing is written unless every town ran: a failing second town leaves no fresh first
/// town beside the stale files of an earlier run (review).
#[test]
fn the_towns_write_nothing_unless_every_town_ran() {
    let data = frozen_town_data("towns-red-map", POOL_D).expect("the town's data");
    let dir = std::env::temp_dir().join(format!("simagent-towns-red-{}", std::process::id()));
    let failing = Plan {
        tag: "no namespace",
        ..t1()
    };
    let runs: [Run; 2] = [
        (t1(), dir.clone(), decided_closed),
        (failing, dir.join("open"), decided_open),
    ];
    let result = towns(&data, &runs);
    let written = dir.exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err(), "{result:?}");
    assert!(!written, "a town was written though the other failed");
}

/// Both towns run on the same data; each one's export and report land in its own directory,
/// and the reds are those of the checks each town decides.
#[test]
fn the_towns_write_each_town_and_split_the_reds() {
    let data = frozen_town_data("towns-map", POOL_D).expect("the town's data");
    let dir = std::env::temp_dir().join(format!("simagent-towns-{}", std::process::id()));
    let runs: [Run; 2] = [
        (t1(), dir.clone(), decided_closed),
        (t1(), dir.join("open"), decided_open),
    ];
    let got = towns(&data, &runs);
    let read = |path: std::path::PathBuf| std::fs::read_to_string(path).expect("written");
    let (closed, open, exports) = (
        read(dir.join("report.json")),
        read(dir.join("open/report.json")),
        [dir.join("export.ndjson"), dir.join("open/export.ndjson")].map(read),
    );
    std::fs::remove_dir_all(&dir).expect("removed");
    let want: Value = serde_json::from_str(T1_REPORT).expect("the reference's report");
    for written in [&closed, &open] {
        let written: Value = serde_json::from_str(written).expect("a report");
        assert_eq!(decisions(&written), decisions(&want));
    }
    assert_eq!(exports[0], exports[1]);
    let mut reds_of = reds(&want, &decided_closed);
    reds_of.extend(reds(&want, &decided_open));
    assert_eq!(got.expect("both ran"), reds_of);
}
