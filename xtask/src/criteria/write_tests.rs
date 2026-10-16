//! The export of the reference's two small pool towns (design note of the criteria kit,
//! section 7, fixture 3): T1 `pool_d`, seed 6, `n_start` 30, no immigration, 60 years (two
//! town generations); T2 `pool_c`, seed 0xff, `n_start` 50, immigration, to the end of year
//! 13 (arrivals with births, pool children, carried couples, one formed at the arrival
//! second). Integers, keys and words exact; `z` and `gl` within `1e-12 · max(1, |x|)`.

use std::collections::BTreeSet;

use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::input::{Input, PoolStart, Start};
use sim_core::params::WorldParams;
use sim_core::pool::take_households;
use sim_core::time::SimTime;

use super::{Observer, write};
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

/// The template of map 4 with the repository's life course data and this pool, as the
/// arrival tests and `tools/arrival_reference.py` build it.
fn town_data(pool: &[u8]) -> Data {
    let root = root();
    let demography =
        crate::map4::read(&root, "data/calibration/demography.json").expect("demography");
    let (calibration, map) =
        crate::prerun::read_files(&root.join("tests/golden/map4_template")).expect("the template");
    let definitions =
        crate::prerun::definitions_of(&demography, &calibration, &map).expect("definitions");
    Data::with_pool(definitions, pool).expect("checked")
}

/// The export of a pool town run from the households of the draw for occasion 0 to `end`.
fn export_of(pool: &[u8], seed: u64, n_start: u64, immigration: bool, end: i64) -> String {
    let data = town_data(pool);
    let held = data.pool().expect("a pool");
    let (taken, _) = take_households(held.pool(), seed, 0, &BTreeSet::new(), n_start);
    let start = Start {
        world_seed: seed,
        founder_tag: DataId::new("test:arrival").expect("id"),
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders: Vec::new(),
        couples: Vec::new(),
        capacity: None,
        pool: Some(PoolStart {
            pool_hash: held.hash(),
            n_start,
            immigration,
            households: taken
                .iter()
                .map(|&index| held.pool().households[index].clone())
                .collect(),
        }),
    };
    let mut observer = Observer::default();
    let end = SimTime::from_secs(end);
    let (world, _) = crate::prerun::run_observed(data, Input::Start(start), end, &mut |entries| {
        observer.observe(entries)
    })
    .expect("a run");
    write(&world, &observer, end).expect("an export")
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
