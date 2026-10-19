//! The export of the reference's two small pool towns (design note of the criteria kit,
//! section 7, fixture 3): T1 `pool_d`, seed 6, `n_start` 30, no immigration, 60 years (two
//! town generations); T2 `pool_c`, seed 0xff, `n_start` 50, immigration, to the end of year
//! 13 (arrivals with births, pool children, carried couples, one formed at the arrival
//! second). Each town on the documents its pool was extracted from. Integers, keys and
//! words exact; `z` and `gl` within `1e-12 · max(1, |x|)`.

use std::collections::{BTreeMap, BTreeSet};

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::input::{Input, Start};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;

use super::{Observer, carried, write};
use crate::criteria::export::{Export, Row, read};

const POOL_C: &[u8] = include_bytes!("../../../tests/golden/pool/pool_c.bin");
const POOL_D: &[u8] = include_bytes!("../../../tests/golden/pool/pool_d.bin");
const T1: &str = include_str!("../../../tests/golden/criteria/t1_pool_d.ndjson");
const T2: &str = include_str!("../../../tests/golden/criteria/t2_pool_c.ndjson");
const YEAR: i64 = 31_449_600;

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace")
        .to_path_buf()
}

/// The frequencies this pool was made with, as `Data::with_pool` takes them (slice S2a of
/// the map-4 switch): the file's (`tests/golden/pool/pool_frequencies.json`) for `pool_c`,
/// those of its pool run `frequent_alleles` for `pool_d` (`tools/pool_reference.py`).
fn frequencies(pool: &[u8]) -> BTreeMap<DataId, FiniteF64> {
    let values: Vec<(String, f64)> = if pool == POOL_D {
        [
            ("core:disease_ar", 0.4),
            ("core:disease_ad_late", 0.3),
            ("core:disease_ad_partial", 0.35),
            ("core:disease_xr", 0.45),
            ("core:disease_mt", 0.25),
            ("core:mc1r", 0.6),
            ("core:herc2", 0.55),
        ]
        .map(|(id, q)| (id.to_owned(), q))
        .to_vec()
    } else {
        let file: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/golden/pool/pool_frequencies.json"
        ))
        .expect("json");
        file["pool_frequencies"]
            .as_object()
            .expect("the entries")
            .iter()
            .map(|(id, entry)| (id.clone(), entry["value"].as_f64().expect("a value")))
            .collect()
    };
    values
        .into_iter()
        .map(|(id, q)| {
            let q = FiniteF64::try_new(q).expect("finite");
            (DataId::new(&id).expect("id"), q)
        })
        .collect()
}

/// This pool on the documents it was extracted from, whose data hash is its `frozen_hash`
/// (design note of the pool's records, section 6), as the arrival tests and
/// `tools/arrival_reference.py` build it: `pool_c` on those of the pool run `father_dies`
/// without `pool_frequencies` (`tests/golden/father_dies`), `pool_d` on the template of
/// map 4 with the repository's life course data; with the frequencies it was made with.
fn town_data(pool: &[u8]) -> Data {
    let root = root();
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
    let demography = crate::map4::read(&root, demography).expect("demography");
    let (calibration, map) =
        crate::prerun::read_files(&root.join(documents)).expect("the documents");
    let definitions =
        crate::prerun::definitions_of(&demography, &calibration, &map).expect("definitions");
    Data::with_pool(definitions, pool, frequencies(pool)).expect("checked")
}

/// A pool town of the reference's (tag `test:arrival`, no capacity) run to `end`.
fn run_town(
    pool: &[u8],
    seed: u64,
    n_start: u64,
    immigration: bool,
    end: i64,
) -> (sim_core::world::World, Observer) {
    let plan = crate::criteria::town::Plan {
        seed,
        tag: "test:arrival",
        n_start,
        capacity: None,
        immigration,
        end: SimTime::from_secs(end),
    };
    crate::criteria::town::run_town(town_data(pool), &plan).expect("a run")
}

/// The export of a pool town to `end`.
fn export_of(pool: &[u8], seed: u64, n_start: u64, immigration: bool, end: i64) -> String {
    let (world, observer) = run_town(pool, seed, n_start, immigration, end);
    write(&world, &observer, SimTime::from_secs(end)).expect("an export")
}

/// Review: the writer refuses an end the world did not close, and a town child whose
/// conception the observer never saw (it would silently write the child's own key as its
/// genome key and no multiple index).
#[test]
fn the_writer_checks_its_end_and_its_observer() {
    let end = 60 * YEAR - 1;
    let (world, observer) = run_town(POOL_D, 6, 30, false, end);
    assert!(write(&world, &observer, SimTime::from_secs(end - 1)).is_err());
    assert!(write(&world, &Observer::default(), SimTime::from_secs(end)).is_err());
    assert!(write(&world, &observer, SimTime::from_secs(end)).is_ok());
}

fn close(a: f64, b: f64) -> bool {
    let scale = if b.abs() > 1.0 { b.abs() } else { 1.0 };
    (a - b).abs() <= 1e-12 * scale
}

/// Everything but `z` and `gl` exact, those two within the census's tolerance.
fn assert_same(rust: &Export, reference: &Export) {
    assert_eq!(rust.header, reference.header, "header");
    assert_eq!(rust.couples, reference.couples, "couples");
    assert_eq!(rust.rows.len(), reference.rows.len(), "persons");
    for (row, want) in rust.rows.iter().zip(&reference.rows) {
        let exact = |row: &Row| Row {
            z: [0.0; 3],
            gl: 0.0,
            ..row.clone()
        };
        assert_eq!(exact(row), exact(want), "person {:016x}", want.key);
        for (z, w) in row.z.iter().zip(&want.z) {
            assert!(close(*z, *w), "z of {:016x}: {z} against {w}", want.key);
        }
        assert!(close(row.gl, want.gl), "gl of {:016x}", want.key);
    }
}

#[test]
fn t1_pool_d_exports_as_the_reference() {
    let text = export_of(POOL_D, 6, 30, false, 60 * YEAR - 1);
    assert_same(
        &read(&text).expect("the Rust export"),
        &read(T1).expect("T1"),
    );
}

#[test]
fn t2_pool_c_exports_as_the_reference() {
    let text = export_of(POOL_C, 0xff, 50, true, 14 * YEAR - 1);
    assert_same(
        &read(&text).expect("the Rust export"),
        &read(T2).expect("T2"),
    );
}

/// The text without the values of `z` and `gl`: a value may differ in its last bits (the
/// pinned libm against a correctly rounded reference, ADR-0003), everything else is the
/// reference's byte for byte, key order and compact form included.
fn masked(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut out = line.to_owned();
            for (open, close) in [("\"z\":[", ']'), ("\"gl\":\"", '"')] {
                if let Some(start) = out.find(open) {
                    let from = start + open.len();
                    let end = from + out[from..].find(close).expect("closed");
                    out.replace_range(from..end, "_");
                }
            }
            out + "\n"
        })
        .collect()
}

#[test]
fn the_export_is_the_references_text() {
    let t1 = export_of(POOL_D, 6, 30, false, 60 * YEAR - 1);
    let t2 = export_of(POOL_C, 0xff, 50, true, 14 * YEAR - 1);
    assert_eq!(masked(&t1), masked(T1));
    assert_eq!(masked(&t2), masked(T2));
    // The mask hides only those values: any other change still shows.
    assert_ne!(
        masked(&t1.replacen("\"pool\":0", "\"pool\":2", 1)),
        masked(T1)
    );
}

/// A town of two `Start` founders on the template without a pool: founders have no
/// conception in the log, so no multiple index, and the export needs none (mutants).
#[test]
fn founders_have_no_multiple_index() {
    let root = root();
    let demography =
        crate::map4::read(&root, "data/calibration/demography.json").expect("demography");
    let (calibration, map) =
        crate::prerun::read_files(&root.join("tests/golden/map4_template")).expect("the template");
    let data = Data::new(
        crate::prerun::definitions_of(&demography, &calibration, &map).expect("definitions"),
    )
    .expect("checked");
    let founder = |sex| sim_core::input::Founder {
        born: SimTime::from_secs(-20 * YEAR),
        sex,
        alleles: Vec::new(),
    };
    let start = Start {
        world_seed: 3,
        founder_tag: DataId::new("test:founders").expect("id"),
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders: vec![
            founder(sim_core::genome::Sex::Female),
            founder(sim_core::genome::Sex::Male),
        ],
        couples: Vec::new(),
        capacity: None,
        pool: None,
    };
    let mut observer = Observer::default();
    let end = SimTime::from_secs(YEAR - 1);
    let (world, _) = crate::prerun::run_observed(data, Input::Start(start), end, &mut |entries| {
        observer.observe(entries)
    })
    .expect("a run");
    let export = read(&write(&world, &observer, end).expect("an export")).expect("read");
    assert_eq!(export.header.pool_seed, None);
    assert_eq!(export.rows.len(), 2);
    for row in &export.rows {
        assert_eq!((row.pool, row.mother, row.mult), (0, None, None));
    }
}

/// `carried` (section 1): the pool held the pair, she arrived, and it was formed by her
/// arrival; each condition alone is not enough (mutants).
#[test]
fn carried_needs_the_pool_pair_and_her_arrival() {
    let tag = DataId::new("test:carried").expect("id");
    let key = |n: u64| sim_core::lineage::LineageKey::founder(&tag, n);
    let pairs: BTreeSet<_> = [(key(1), key(2))].into_iter().collect();
    let at = |s: i64| Some(SimTime::from_secs(s));
    assert!(
        carried(&pairs, at(10), key(1), key(2), SimTime::from_secs(10)),
        "at her arrival"
    );
    assert!(
        carried(&pairs, at(10), key(1), key(2), SimTime::from_secs(-5)),
        "before"
    );
    assert!(
        !carried(&pairs, at(10), key(1), key(2), SimTime::from_secs(11)),
        "formed again later"
    );
    assert!(
        !carried(&pairs, at(10), key(1), key(3), SimTime::from_secs(10)),
        "the market at her arrival"
    );
    assert!(
        !carried(&pairs, None, key(1), key(2), SimTime::from_secs(10)),
        "she did not arrive"
    );
}
