//! The body in the yearly census (ADR-0020 section 7, ADR-0005 section 8; design note of the
//! census): every living sim evaluated at the year end and counted per clamped parameter,
//! against the independent reference `tools/census_reference.py`. Its fixtures (golden
//! `tests/golden/census/`):
//! - (a) the template of map 4, 27 founders naming every locus (low, high and mixed
//!   genomes), two nutrition shocks, 20 years: 19 of the 21 keys clamp, the growth curve
//!   holds three founders every year;
//! - (b) "arrival" on `pool_c` with three shocks: an arrival does without the years before
//!   its arrival (G6), to the exact boundary;
//! - (b2) `pool_b` founded at the start, without immigration;
//! - (c) the small pre-run run: data without body traits, so every count is 0.
//!
//! Each row is compared exactly: counts are integers, and no raw value lies near a bound
//! (the reference's nearest is 2.3e-5 away, its nearest curve input 0.19).
//!
//! At the snapshots of (a), (b) and (b2) every living person's 44 body values are compared
//! within `1e-12 · max(1, |reference|)`, absolute below 1 and relative above. Bit equality
//! is not promised: `sim-math` keeps `exp` and its relatives within 1 ulp of the correctly
//! rounded value the reference uses (ADR-0003), and the mapping chains several of them
//! (1 to 3 ulp differences occur in `body_hair`). The bound is not derived from those
//! chains; it is set many orders of magnitude above them (about 4500 ulp near 1) and many
//! below a change of any calibration value, whose fourth digit moves a value by about 1e-4.

mod common;

use std::collections::BTreeSet;

use serde_json::Value;
use sim_core::body_contract::{BodyParamKey, CLAMPED_KEYS};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::hash::stable_hash64;
use sim_core::ids::InputSeq;
use sim_core::input::{Couple, Founder, Input, InputRecord, PoolStart, Start};
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::{NutritionYear, WorldParams};
use sim_core::pool::{Pool, pool_loci, take_households};
use sim_core::time::SimTime;
use sim_core::world::{World, YearOut};

const CENSUS: &str = include_str!("../../../tests/golden/census/census.json");
const BODIES: &str = include_str!("../../../tests/golden/census/bodies.json");
const POOL_B: &[u8] = include_bytes!("../../../tests/golden/pool/pool_b.bin");
const POOL_C: &[u8] = include_bytes!("../../../tests/golden/pool/pool_c.bin");

/// The fixtures: their name, scenario and data.
fn fixtures() -> [(&'static str, &'static str, Data); 4] {
    [
        (
            "a_template_extremes",
            include_str!("../../../tests/golden/census/a_template_extremes.json"),
            template_data(),
        ),
        (
            "b_pool_c_immigration",
            include_str!("../../../tests/golden/census/b_pool_c_immigration.json"),
            pool_data(POOL_C),
        ),
        (
            "b2_pool_b_start",
            include_str!("../../../tests/golden/census/b2_pool_b_start.json"),
            pool_data(POOL_B),
        ),
        (
            "c_prerun_small",
            include_str!("../../../tests/golden/census/c_prerun_small.json"),
            prerun_data(),
        ),
    ]
}

fn template() -> (Value, Value) {
    common::files_from(
        include_str!("../../../tests/golden/map4_template/calibration/genetics.json"),
        include_str!("../../../tests/golden/map4_template/genetics/genome_map.json"),
    )
}

fn template_data() -> Data {
    let (calibration, genome_map) = template();
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("data")
}

fn pool_data(pool: &[u8]) -> Data {
    let (calibration, genome_map) = template();
    Data::with_pool(
        common::definitions(calibration, genome_map).expect("definitions"),
        pool,
    )
    .expect("data")
}

/// The pre-run's data: the template without the seven loci the pool carries and without
/// the diseases of a single locus.
fn prerun_data() -> Data {
    let (mut calibration, mut genome_map) = template();
    let pool: Vec<String> = pool_loci()
        .iter()
        .map(|id| id.as_str().to_owned())
        .collect();
    genome_map["loci"]
        .as_array_mut()
        .expect("loci")
        .retain(|locus| !pool.iter().any(|id| locus["id"] == id.as_str()));
    calibration["diseases"]
        .as_array_mut()
        .expect("diseases")
        .retain(|disease| disease["carrier"].get("locus").is_none());
    Data::new(common::definitions(calibration, genome_map).expect("definitions")).expect("data")
}

fn hex_u64(value: &Value) -> u64 {
    u64::from_str_radix(value.as_str().expect("hex"), 16).expect("hex digits")
}

/// The `Start` of a scenario of format 5 on `data`.
fn start_of(scenario: &Value, data: &Data) -> Start {
    let start = &scenario["start"];
    let seed = hex_u64(&start["world_seed"]);
    let nutrition = start["world_params"]["nutrition"]
        .as_array()
        .expect("nutrition")
        .iter()
        .map(|year| NutritionYear {
            year: year["year"].as_i64().expect("year"),
            index: FiniteF64::try_new(year["index"].as_f64().expect("index")).expect("finite"),
        })
        .collect();
    let founders = start["founders"]
        .as_array()
        .expect("founders")
        .iter()
        .map(|founder| Founder {
            born: SimTime::from_secs(founder["born"].as_i64().expect("born")),
            sex: match founder["sex"].as_str().expect("sex") {
                "female" => Sex::Female,
                _ => Sex::Male,
            },
            alleles: founder["alleles"]
                .as_array()
                .expect("alleles")
                .iter()
                .map(|locus| LocusAlleles {
                    locus: DataId::new(locus["locus"].as_str().expect("locus")).expect("id"),
                    copies: locus["copies"]
                        .as_array()
                        .expect("copies")
                        .iter()
                        .map(|c| u8::try_from(c.as_u64().expect("copy")).expect("small"))
                        .collect(),
                })
                .collect(),
        })
        .collect();
    let couples = start["couples"]
        .as_array()
        .expect("couples")
        .iter()
        .map(|couple| Couple {
            woman: u32::try_from(couple["woman"].as_u64().expect("woman")).expect("small"),
            man: u32::try_from(couple["man"].as_u64().expect("man")).expect("small"),
        })
        .collect();
    let pool = start
        .get("pool")
        .filter(|pool| !pool.is_null())
        .map(|pool| {
            let n_start = pool["n_start"].as_u64().expect("n_start");
            let held = data.pool().expect("a pool");
            let (taken, _) = take_households(held.pool(), seed, 0, &BTreeSet::new(), n_start);
            PoolStart {
                pool_hash: held.hash(),
                n_start,
                immigration: pool["immigration"].as_bool().expect("immigration"),
                households: taken
                    .iter()
                    .map(|&index| held.pool().households[index].clone())
                    .collect(),
            }
        });
    Start {
        world_seed: seed,
        founder_tag: DataId::new(start["founder_tag"].as_str().expect("tag")).expect("id"),
        world_params: WorldParams {
            hereditary_diseases: start["world_params"]["hereditary_diseases"]
                .as_bool()
                .expect("switch"),
            nutrition,
        },
        founders,
        couples,
        capacity: start.get("capacity").and_then(Value::as_u64),
        pool,
    }
}

/// Every living person's 44 body values at `t`, by ascending SimId.
fn bodies(world: &World, t: SimTime) -> Vec<(u64, Vec<f64>)> {
    world
        .pedigree()
        .persons
        .iter()
        .filter(|person| person.died.is_none() && person.emigrated.is_none())
        .map(|person| {
            let body = world.state().body_at(person.sim, t).expect("a body");
            let values = BodyParamKey::ALL
                .iter()
                .map(|key| body.params.get(*key))
                .collect();
            (person.sim.get(), values)
        })
        .collect()
}

/// The years of the snapshots each fixture must have.
fn snapshot_years(name: &str) -> Vec<i64> {
    match name {
        "a_template_extremes" => vec![0, 7, 19],
        "b_pool_c_immigration" => vec![0, 9, 13],
        "b2_pool_b_start" => vec![0, 9, 19],
        _ => Vec::new(),
    }
}

/// Equal within `1e-12 · max(1, |reference|)` (see the module's head).
fn close(ours: f64, reference: f64) -> bool {
    let scale = if reference.abs() > 1.0 {
        reference.abs()
    } else {
        1.0
    };
    (ours - reference).abs() <= 1e-12 * scale
}

#[test]
fn the_keys_are_the_references() {
    let census: Value = serde_json::from_str(CENSUS).expect("json");
    let names = |list: &Value| -> Vec<String> {
        list.as_array()
            .expect("keys")
            .iter()
            .map(|key| key.as_str().expect("name").to_owned())
            .collect()
    };
    let clamped: Vec<String> = CLAMPED_KEYS.iter().map(|k| k.as_str().to_owned()).collect();
    assert_eq!(names(&census["clamped_keys"]), clamped);
    let all: Vec<String> = BodyParamKey::ALL
        .iter()
        .map(|k| k.as_str().to_owned())
        .collect();
    assert_eq!(names(&census["param_keys"]), all);
    let bodies: Value = serde_json::from_str(BODIES).expect("json");
    assert_eq!(names(&bodies["param_keys"]), all);
}

/// Each fixture as the reference: its data, its `Start`, every census row, the year and
/// log hashes, and the snapshots of the bodies.
#[test]
fn the_census_counts_the_bodies_as_the_reference() {
    let census: Value = serde_json::from_str(CENSUS).expect("json");
    let bodies_golden: Value = serde_json::from_str(BODIES).expect("json");
    let bodies_golden = &bodies_golden["fixtures"];
    for (name, scenario, data) in fixtures() {
        let expected = &census["fixtures"][name];
        let scenario: Value = serde_json::from_str(scenario).expect("scenario");
        assert_eq!(data.hash(), hex_u64(&expected["data_hash"]), "{name}: data");
        let start = start_of(&scenario, &data);
        let record = postcard::to_allocvec(&InputRecord {
            t: SimTime::from_secs(-1),
            seq: InputSeq::try_from(1).expect("valid"),
            input: Input::Start(start.clone()),
        })
        .expect("encodes");
        assert_eq!(
            stable_hash64(&record),
            hex_u64(&expected["start_input_hash"]),
            "{name}: start"
        );
        let mut world = World::new(data);
        world.submit(Input::Start(start)).expect("accepted");
        let mut log: Vec<LogEntry> = world.take_log();
        let mut out = YearOut::default();
        let snapshots = bodies_golden[name].as_array().cloned().unwrap_or_default();
        let years: Vec<i64> = snapshots
            .iter()
            .map(|shot| shot["year"].as_i64().expect("year"))
            .collect();
        assert_eq!(years, snapshot_years(name), "{name}: snapshots");
        for shot in snapshots {
            let t = SimTime::from_secs(shot["t"].as_i64().expect("t"));
            world.advance_until(&mut out, t, &mut |_| false);
            log.extend(world.take_log());
            let ours = bodies(&world, t);
            let reference = shot["persons"].as_array().expect("persons");
            assert_eq!(
                ours.len(),
                reference.len(),
                "{name}: living at {}",
                shot["year"]
            );
            for ((sim, values), person) in ours.iter().zip(reference) {
                assert_eq!(*sim, person[0].as_u64().expect("sim"), "{name}");
                assert_eq!(person[1].as_array().expect("values").len(), 44, "{name}");
                for ((key, ours), bits) in BodyParamKey::ALL
                    .iter()
                    .zip(values)
                    .zip(person[1].as_array().expect("values"))
                {
                    let reference = f64::from_bits(hex_u64(bits));
                    assert!(
                        close(*ours, reference),
                        "{name}: sim {sim} {} at {}: {ours} against {reference}",
                        key.as_str(),
                        shot["year"]
                    );
                }
            }
        }
        let until = SimTime::from_secs(scenario["until"].as_i64().expect("until"));
        world.advance_until(&mut out, until, &mut |_| false);
        log.extend(world.take_log());
        let rows: Vec<Value> = out
            .census
            .iter()
            .map(|row| {
                serde_json::json!({
                    "year": row.year,
                    "living": row.living,
                    "lived_secs": row.lived_secs,
                    "body_evaluated": row.body.evaluated,
                    "clamped": row.body.clamped.to_vec(),
                    "curve_clamped": row.body.curve_clamped,
                })
            })
            .collect();
        assert_eq!(Value::Array(rows), expected["rows"], "{name}: rows");
        let hashes: Vec<u64> = out.year_hashes.iter().map(|y| y.hash).collect();
        let reference: Vec<u64> = expected["year_hashes"]
            .as_array()
            .expect("year hashes")
            .iter()
            .map(hex_u64)
            .collect();
        assert_eq!(hashes, reference, "{name}: year hashes");
        assert_eq!(
            log_hash(&log),
            hex_u64(&expected["log_hash"]),
            "{name}: log"
        );
    }
}

/// The census of data without body traits is all zero; with body traits it evaluates every
/// living sim once (design note of the census, 3.6: cadence).
#[test]
fn every_living_sim_is_evaluated_once_a_year() {
    let census: Value = serde_json::from_str(CENSUS).expect("json");
    for (name, _, _) in fixtures() {
        for row in census["fixtures"][name]["rows"].as_array().expect("rows") {
            let evaluated = row["body_evaluated"].as_u64().expect("evaluated");
            let living = row["living"].as_u64().expect("living");
            if name == "c_prerun_small" {
                assert_eq!(evaluated, 0, "{name}");
            } else {
                assert_eq!(evaluated, living, "{name}");
            }
        }
    }
    let _ = Pool::from_bytes(POOL_B).expect("pool_b");
}
