//! The pool file against `tools/pool_reference.py` (design note of the pool file): the
//! frequencies' data check, and five small pool runs on the template of map 4 whose files,
//! hashes and genome keys the reference wrote (`tests/golden/pool/`). The reference was
//! written from the design note without this code.

use super::*;
use crate::ordered::Json;
use sim_core::lineage::LineageKey;

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace")
        .to_path_buf()
}

fn read(path: &str) -> Json {
    crate::map4::read(&root(), path).expect("a file")
}

/// The template's calibration with the entry `pool_frequencies` as the reference writes it,
/// its map and the repository's life course data.
fn documents() -> (Json, Json, Json) {
    let mut calibration = read("tests/golden/map4_template/calibration/genetics.json");
    let entry = read("tests/golden/pool/pool_frequencies.json");
    calibration.set(
        "pool_frequencies",
        entry.get("pool_frequencies").expect("the entry").clone(),
    );
    (
        calibration,
        read("tests/golden/map4_template/genetics/genome_map.json"),
        read("data/calibration/demography.json"),
    )
}

/// The `q` of the entry, bit for bit (`pool_frequency` lines of the reference).
const FREQUENCY_BITS: [(&str, u64); 7] = [
    ("core:disease_ar", 0x3f94_7ae1_47ae_147b),
    ("core:disease_ad_late", 0x3eff_7510_4d55_1d69),
    ("core:disease_ad_partial", 0x3f50_624d_d2f1_a9fc),
    ("core:disease_xr", 0x3f30_624d_d2f1_a9fc),
    ("core:disease_mt", 0x3f1f_7510_4d55_1d69),
    ("core:mc1r", 0x3fc3_3333_3333_3333),
    ("core:herc2", 0x3fe8_0000_0000_0000),
];

/// The entry passes and is removed; its `q` are the reference's.
#[test]
fn the_frequencies_are_read_and_removed() {
    let (calibration, map, _) = documents();
    let (rest, frequencies) = take_frequencies(&calibration, &map).expect("accepted");
    assert!(rest.get("pool_frequencies").is_none());
    assert_eq!(
        rest,
        read("tests/golden/map4_template/calibration/genetics.json")
    );
    for (id, bits) in FREQUENCY_BITS {
        let q = frequencies[&DataId::new(id).expect("id")];
        assert_eq!(q.to_bits(), bits, "{id}");
    }
}

/// A rejected case: its name, the change of the entry, the rule.
type BadEntry = (&'static str, Box<dyn Fn(&mut Json)>, &'static str);

/// The seven cases the reference rejects (`pool_frequencies_rejected`), each with its rule.
#[test]
fn a_bad_entry_is_rejected_by_its_rule() {
    let (calibration, map, _) = documents();
    let entry = |calibration: &Json| {
        calibration
            .get("pool_frequencies")
            .expect("the entry")
            .clone()
    };
    let with = |change: &dyn Fn(&mut Json)| {
        let mut calibration = calibration.clone();
        let mut changed = entry(&calibration);
        change(&mut changed);
        calibration.set("pool_frequencies", changed);
        take_frequencies(&calibration, &map).expect_err("rejected")
    };
    let set_q = |q: f64| {
        move |e: &mut Json| {
            e.get_mut("core:disease_ar")
                .expect("ar")
                .set("value", Json::float(q));
        }
    };
    let cases: [BadEntry; 7] = [
        (
            "missing_locus",
            Box::new(|e: &mut Json| {
                if let Json::Object(members) = e {
                    members.retain(|(id, _)| id != "core:herc2");
                }
            }),
            "pool.frequencies_loci",
        ),
        (
            "extra_locus",
            Box::new(|e: &mut Json| {
                let copy = e.get("core:herc2").expect("herc2").clone();
                e.set("core:other", copy);
            }),
            "pool.frequencies_loci",
        ),
        ("q_zero", Box::new(set_q(0.0)), "pool.frequency_range"),
        ("q_one", Box::new(set_q(1.0)), "pool.frequency_range"),
        ("q_negative", Box::new(set_q(-0.1)), "pool.frequency_range"),
        ("q_above_one", Box::new(set_q(1.5)), "pool.frequency_range"),
        (
            "unit_ratio",
            Box::new(|e: &mut Json| {
                e.get_mut("core:mc1r")
                    .expect("mc1r")
                    .set("unit", Json::str("ratio"));
            }),
            "pool.frequency_unit",
        ),
    ];
    for (case, change, rule) in cases {
        let error = with(&*change);
        assert!(error.starts_with(rule), "{case}: {error}");
    }
}

/// A pool run of the reference: its founders and years, the changes of its scenario, the
/// hashes of its data and its pool.
struct Run {
    name: &'static str,
    founders: u64,
    years: i64,
    scenario: Scenario,
    run_data_hash: u64,
    frozen_data_hash: u64,
    pool_hash: u64,
    file: &'static [u8],
}

#[derive(Clone, Copy)]
enum Scenario {
    Natural,
    /// The threshold disease at prevalence 0.3 killing at 1 per year, `c` 0.2, identical
    /// twins 0.2 (NOTES of the reference, section 7).
    FatherDies,
    /// Only the frequencies change.
    FrequentAlleles,
}

const RUNS: [Run; 5] = [
    Run {
        name: "pool_a",
        founders: 60,
        years: 60,
        scenario: Scenario::Natural,
        run_data_hash: 0xb17a_fdcd_2961_75ac,
        frozen_data_hash: 0x5ae3_d70c_71cf_063a,
        pool_hash: 0x6488_c5c7_c9a7_40ef,
        file: include_bytes!("../../tests/golden/pool/pool_a.bin"),
    },
    Run {
        name: "pool_b",
        founders: 40,
        years: 30,
        scenario: Scenario::Natural,
        run_data_hash: 0xb17a_fdcd_2961_75ac,
        frozen_data_hash: 0x5ae3_d70c_71cf_063a,
        pool_hash: 0x28af_470e_4c30_38f4,
        file: include_bytes!("../../tests/golden/pool/pool_b.bin"),
    },
    Run {
        name: "pool_c",
        founders: 80,
        years: 2,
        scenario: Scenario::FatherDies,
        run_data_hash: 0xeb9f_b406_aa14_2f6a,
        frozen_data_hash: 0xe2f2_44a7_e4cd_db5f,
        pool_hash: 0xe4a7_3ea3_34e7_4b6c,
        file: include_bytes!("../../tests/golden/pool/pool_c.bin"),
    },
    Run {
        name: "pool_d",
        founders: 60,
        years: 60,
        scenario: Scenario::FrequentAlleles,
        run_data_hash: 0xb17a_fdcd_2961_75ac,
        frozen_data_hash: 0x5ae3_d70c_71cf_063a,
        pool_hash: 0x979f_8665_a74a_4b1e,
        file: include_bytes!("../../tests/golden/pool/pool_d.bin"),
    },
    Run {
        name: "pool_e",
        founders: 60,
        years: 81,
        scenario: Scenario::Natural,
        run_data_hash: 0xb17a_fdcd_2961_75ac,
        frozen_data_hash: 0x5ae3_d70c_71cf_063a,
        pool_hash: 0x4556_05ea_f7c2_5849,
        file: include_bytes!("../../tests/golden/pool/pool_e.bin"),
    },
];

impl Run {
    fn documents(&self) -> (Json, Json, Json) {
        let (mut calibration, map, mut demography) = documents();
        match self.scenario {
            Scenario::Natural => {}
            Scenario::FatherDies => {
                if let Some(Json::Array(diseases)) = calibration.get_mut("diseases") {
                    for disease in diseases {
                        if disease.get("code").and_then(Json::as_str)
                            == Some("core:disease_liability")
                        {
                            let threshold = disease
                                .get_mut("carrier")
                                .and_then(|c| c.get_mut("threshold"))
                                .expect("a threshold");
                            threshold
                                .get_mut("prevalence")
                                .expect("prevalence")
                                .set("value", Json::float(0.3));
                            disease
                                .get_mut("effect")
                                .and_then(|e| e.get_mut("death"))
                                .and_then(|d| d.get_mut("per_year"))
                                .expect("a rate")
                                .set("value", Json::float(1.0));
                        }
                    }
                }
                let values = demography.get_mut("values").expect("values");
                for (code, value) in [
                    ("core:market_search_height_female", 0.2),
                    ("core:twins_identical", 0.2),
                ] {
                    values
                        .get_mut(code)
                        .expect("a value")
                        .set("value", Json::float(value));
                }
            }
            Scenario::FrequentAlleles => {
                let entry = calibration.get_mut("pool_frequencies").expect("the entry");
                for (id, q) in [
                    ("core:disease_ar", 0.4),
                    ("core:disease_ad_late", 0.3),
                    ("core:disease_ad_partial", 0.35),
                    ("core:disease_xr", 0.45),
                    ("core:disease_mt", 0.25),
                    ("core:mc1r", 0.6),
                    ("core:herc2", 0.55),
                ] {
                    entry
                        .get_mut(id)
                        .expect("a locus")
                        .set("value", Json::float(q));
                }
            }
        }
        (calibration, map, demography)
    }
}

/// The genome keys of the reference for `run`: (child, genome key). A lineage key reads
/// from its JSON form, 16 hex digits.
fn genome_keys(run: &str) -> Vec<(LineageKey, u64)> {
    include_str!("../../tests/golden/pool/genome_keys.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(' ').collect();
            (fields.first() == Some(&run)).then(|| {
                let child: LineageKey =
                    serde_json::from_str(&format!("\"{}\"", fields[1])).expect("a key");
                (child, u64::from_str_radix(fields[2], 16).expect("hex"))
            })
        })
        .collect()
}

/// Each run's pool file is the reference's byte for byte (who is in it, the alleles of the
/// seven loci, the afflictions, the fields, the ancestors), with the data hashes and the
/// genome keys of every conceived child.
#[test]
fn the_pool_files_are_the_references() {
    for run in &RUNS {
        let (calibration, map, demography) = run.documents();
        let (rest, _) = take_frequencies(&calibration, &map).expect("accepted");
        let frozen = data_of(&demography, &rest, &map).expect("data");
        assert_eq!(frozen.hash(), run.frozen_data_hash, "{}", run.name);
        let (pool, _, keys) = pool_of(
            &demography,
            &calibration,
            &map,
            run.founders,
            run.years,
            &mut |_, _, _| Ok(()),
        )
        .expect("a pool");
        assert_eq!(pool.data_hash, run.run_data_hash, "{}", run.name);
        let expected_keys = genome_keys(run.name);
        assert!(!expected_keys.is_empty(), "{}", run.name);
        for (child, genome) in expected_keys {
            let found = keys.of(child);
            assert_eq!(
                found.get(),
                genome,
                "{} child {:016x}",
                run.name,
                child.get()
            );
        }
        let bytes = pool.to_bytes();
        assert!(bytes == run.file, "{}: the file differs", run.name);
        assert_eq!(pool.hash(), run.pool_hash, "{}", run.name);
    }
}
