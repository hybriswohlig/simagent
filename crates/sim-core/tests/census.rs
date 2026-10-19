//! The body in the yearly census (ADR-0020 section 7, ADR-0005 section 8; design note of the
//! census): every living sim evaluated at the year end and counted per clamped parameter,
//! against the independent reference `tools/census_reference.py`. Its fixtures (golden
//! `tests/golden/census/`):
//! - (a) the template of map 4, 27 founders naming every locus (low, high and mixed
//!   genomes), two nutrition shocks, 20 years: 19 of the 21 keys clamp, the growth curve
//!   holds three founders every year;
//! - (b) "arrival" on `pool_c` with three shocks: an arrival does without the years before
//!   its arrival (G6), to the exact boundary; on the documents `pool_c` was extracted from
//!   (`tests/golden/father_dies`, design note of the pool's records, section 6);
//! - (b2) `pool_b` founded at the start, without immigration;
//! - (c) the small pre-run run: data without MC1R and HERC2, read as the reference allele
//!   (design note of the census 6.1), so every living body is evaluated.
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
use sim_core::body_contract::{BodyData, BodyInputs, BodyParamKey, CLAMPED_KEYS, body_params};
use sim_core::data::Data;
use sim_core::data::DataError;
use sim_core::data_id::DataId;
use sim_core::finite::FiniteF64;
use sim_core::genome::Genome;
use sim_core::genome::{LocusAlleles, Sex};
use sim_core::hash::stable_hash64;
use sim_core::ids::{InputSeq, SimId};
use sim_core::input::{ArrangePartnership, Couple, Founder, Input, InputRecord, PoolStart, Start};
use sim_core::lineage::LineageKey;
use sim_core::log::{LogEntry, log_hash};
use sim_core::params::{NutritionYear, WorldParams};
use sim_core::phenotype::Environment;
use sim_core::pool::{Pool, pool_loci, take_households};
use sim_core::rng::{Draws, WorldSeed};
use sim_core::time::{SimDuration, SimTime};
use sim_core::trace::{TraceFilter, TracePayload};
use sim_core::world::{World, YearOut};

const CENSUS: &str = include_str!("../../../tests/golden/census/census.json");
const BODIES: &str = include_str!("../../../tests/golden/census/bodies.json");
const PHENOTYPES: &str = include_str!("../../../tests/golden/census/phenotypes.json");
const PURE_HEIGHTS: &str = include_str!("../../../tests/golden/census/pure_heights.json");
const RECORDS_PHENOTYPES: &str =
    include_str!("../../../tests/golden/census/records_phenotypes.json");
const POOL_B: &[u8] = include_bytes!("../../../tests/golden/pool/pool_b.bin");
const POOL_C: &[u8] = include_bytes!("../../../tests/golden/pool/pool_c.bin");
const POOL_D: &[u8] = include_bytes!("../../../tests/golden/pool/pool_d.bin");

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
            pool_c_data(),
        ),
        (
            "b2_pool_b_start",
            include_str!("../../../tests/golden/census/b2_pool_b_start.json"),
            pool_data("pool_b", POOL_B),
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

/// The template with the golden pool `name` of these bytes (`pool_b`, `pool_d`) and the
/// frequencies it was made with.
fn pool_data(name: &str, pool: &[u8]) -> Data {
    let (calibration, genome_map) = template();
    Data::with_pool(
        common::definitions(calibration, genome_map).expect("definitions"),
        pool,
        common::pool_frequencies(name),
    )
    .expect("data")
}

/// `pool_c` on the documents it was extracted from: those of the pool run `father_dies`
/// without `pool_frequencies` (`tests/golden/father_dies`), whose data hash is the pool's
/// `frozen_hash`; with the frequencies of the file, which `pool_c` was made with.
fn pool_c_data() -> Data {
    let (calibration, genome_map) = common::files_from(
        include_str!("../../../tests/golden/father_dies/calibration/genetics.json"),
        include_str!("../../../tests/golden/father_dies/genetics/genome_map.json"),
    );
    let demography = common::demography_from(include_str!(
        "../../../tests/golden/father_dies/calibration/demography.json"
    ));
    Data::with_pool(
        common::definitions_with(demography, calibration, genome_map).expect("definitions"),
        POOL_C,
        common::pool_frequencies("pool_c"),
    )
    .expect("data")
}

/// The pre-run's data: the template without the seven loci the pool carries and without
/// the diseases of a single locus.
fn prerun_data() -> Data {
    prerun_data_with(&[])
}

/// The pre-run's data with the pool loci of `kept` left in the map.
fn prerun_data_with(kept: &[&str]) -> Data {
    let (mut calibration, mut genome_map) = template();
    let pool: Vec<String> = pool_loci()
        .iter()
        .map(|id| id.as_str().to_owned())
        .filter(|id| !kept.contains(&id.as_str()))
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
                village: false,
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

/// The presentation state (ADR-0021 section 3) on the fixtures with snapshots: at each
/// snapshot every living person's view shows the reference's body at that second; a person
/// who left shows the body of the second it left, and its body at any second is the same
/// before and after it left (the archive gives the mapping the same inputs); the bodies at
/// the reached ages are those of their seconds. Taking the views changes nothing: a twin
/// that never takes them ends with the same year hashes and log (the null-effect twin).
#[test]
fn the_presentation_shows_the_references_bodies() {
    let bodies_golden: Value = serde_json::from_str(BODIES).expect("json");
    let twins = fixtures().into_iter().zip(fixtures());
    // Over all fixtures: people who were seen living and left.
    let mut left = 0;
    for ((name, scenario, data), (_, _, twin_data)) in twins {
        let snapshots = bodies_golden["fixtures"][name]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if snapshots.is_empty() {
            continue;
        }
        let scenario: Value = serde_json::from_str(scenario).expect("scenario");
        let until = SimTime::from_secs(scenario["until"].as_i64().expect("until"));
        let start = start_of(&scenario, &data);
        let mut world = World::new(data);
        let mut twin = World::new(twin_data);
        world.submit(Input::Start(start.clone())).expect("accepted");
        twin.submit(Input::Start(start)).expect("accepted");
        let (mut out, mut twin_out) = (YearOut::default(), YearOut::default());
        let (mut log, mut twin_log) = (world.take_log(), twin.take_log());
        // The body of each person at `until`, while it lives.
        let mut before: Vec<(SimId, Vec<f64>)> = Vec::new();
        let values = |world: &World, sim: SimId, t: SimTime| -> Vec<f64> {
            let body = world.state().body_at(sim, t).expect("a body");
            BodyParamKey::ALL
                .iter()
                .map(|key| body.params.get(*key))
                .collect()
        };
        for shot in snapshots {
            let t = SimTime::from_secs(shot["t"].as_i64().expect("t"));
            world.advance_until(&mut out, t, &mut |_| false);
            twin.advance_until(&mut twin_out, t, &mut |_| false);
            log.extend(world.take_log());
            twin_log.extend(twin.take_log());
            assert_eq!(world.closed_through(), t, "{name}");
            for person in shot["persons"].as_array().expect("persons") {
                let sim = SimId::try_from(person[0].as_u64().expect("sim")).expect("an id");
                let view = world.person_view(sim).expect("a view");
                assert_eq!(
                    (view.shown.at.t, view.ended),
                    (t, None),
                    "{name}: sim {sim:?}"
                );
                // Before its birth a person has no body.
                let unborn = view
                    .born
                    .t
                    .checked_sub(SimDuration::from_secs(1))
                    .expect("a second");
                assert!(
                    world.state().body_at(sim, unborn).is_none(),
                    "{name}: sim {sim:?}"
                );
                for (key, bits) in BodyParamKey::ALL
                    .iter()
                    .zip(person[1].as_array().expect("values"))
                {
                    let reference = f64::from_bits(hex_u64(bits));
                    let ours = view.shown.body.get(*key);
                    assert!(
                        close(ours, reference),
                        "{name}: sim {sim:?} {}: {ours} against {reference}",
                        key.as_str()
                    );
                }
                for shown in &view.at_ages {
                    let body = world
                        .state()
                        .body_at(sim, shown.at.t)
                        .expect("a body")
                        .params;
                    assert_eq!(
                        shown.body, body,
                        "{name}: sim {sim:?} at {}",
                        shown.at.age_years
                    );
                    assert_eq!(
                        shown.at.t.checked_since(view.born.t).map(|a| a.as_secs()),
                        Some(shown.at.age_years * 31_449_600)
                    );
                }
                before.push((sim, values(&world, sim, until)));
            }
        }
        world.advance_until(&mut out, until, &mut |_| false);
        twin.advance_until(&mut twin_out, until, &mut |_| false);
        log.extend(world.take_log());
        twin_log.extend(twin.take_log());
        for person in world.pedigree().persons {
            let Some(end) = person.died.or(person.emigrated) else {
                continue;
            };
            let view = world.person_view(person.sim).expect("a view");
            assert_eq!(view.shown.at.t, end, "{name}: sim {:?}", person.sim);
            assert!(view.ended.is_some(), "{name}: sim {:?}", person.sim);
            if let Some((_, kept)) = before.iter().rev().find(|(sim, _)| *sim == person.sim) {
                assert_eq!(
                    &values(&world, person.sim, until),
                    kept,
                    "{name}: sim {:?}",
                    person.sim
                );
                left += 1;
            }
        }
        let hashes = |out: &YearOut| out.year_hashes.iter().map(|y| y.hash).collect::<Vec<u64>>();
        assert_eq!(hashes(&out), hashes(&twin_out), "{name}: year hashes");
        assert_eq!(log_hash(&log), log_hash(&twin_log), "{name}: log");
    }
    assert!(left > 0, "nobody who was seen living left");
}

/// The census evaluates every living sim once a year (design note of the census, 3.6:
/// cadence), the pre-run's too (6.1).
#[test]
fn every_living_sim_is_evaluated_once_a_year() {
    let census: Value = serde_json::from_str(CENSUS).expect("json");
    for (name, _, _) in fixtures() {
        for row in census["fixtures"][name]["rows"].as_array().expect("rows") {
            let evaluated = row["body_evaluated"].as_u64().expect("evaluated");
            let living = row["living"].as_u64().expect("living");
            assert_eq!(evaluated, living, "{name}");
        }
    }
    let _ = Pool::from_bytes(POOL_B).expect("pool_b");
}

/// The trace of a held growth curve (ADR-0012 section 4, design note of the census 3.2):
/// with every sim traced, (a) gives at each year end exactly the reference's held sims with
/// `z_h` and `z_p` before the hold; the trace changes no year hash; untraced, none comes.
#[test]
fn a_held_curve_is_traced_for_a_traced_sim() {
    let census: Value = serde_json::from_str(CENSUS).expect("json");
    let expected = &census["fixtures"]["a_template_extremes"];
    let (_, scenario, _) = fixtures().into_iter().next().expect("a");
    let scenario: Value = serde_json::from_str(scenario).expect("scenario");
    let until = SimTime::from_secs(scenario["until"].as_i64().expect("until"));
    let run = |filter: TraceFilter| {
        let data = template_data();
        let start = start_of(&scenario, &data);
        let mut world = World::new(data);
        world.set_trace(filter);
        world.submit(Input::Start(start)).expect("accepted");
        let mut out = YearOut::default();
        world.advance_until(&mut out, until, &mut |_| false);
        // The year's output is handed into the buffer: nothing stays behind.
        assert!(out.trace.is_empty());
        let trace = world.take_trace();
        // Events' and census entries in the order of time over all years, the events of a
        // second before the census of that second.
        assert!(trace.windows(2).all(|pair| pair[0].t <= pair[1].t));
        for pair in trace.windows(2) {
            if pair[0].t == pair[1].t {
                let census = |e: &sim_core::trace::TraceEntry| {
                    matches!(e.payload, TracePayload::CurveInputClamped { .. })
                };
                assert!(!census(&pair[0]) || census(&pair[1]), "event after census");
            }
        }
        let held: Vec<(i64, u64, f64, f64)> = trace
            .into_iter()
            .filter_map(|entry| match entry.payload {
                TracePayload::CurveInputClamped { z_h, z_p } => {
                    Some((entry.t.as_secs(), entry.sim.get(), z_h, z_p))
                }
                TracePayload::BirthdayPlanned { .. } => None,
            })
            .collect();
        let hashes: Vec<u64> = out.year_hashes.iter().map(|y| y.hash).collect();
        (held, hashes)
    };
    let (held, hashes) = run(TraceFilter::All);
    let mut reference = Vec::new();
    for (year, sims) in expected["curve_clamped_sims"].as_object().expect("by year") {
        let year: i64 = year.parse().expect("a year");
        for sim in sims.as_array().expect("sims") {
            reference.push((
                (year + 1) * 31_449_600 - 1,
                sim[0].as_u64().expect("sim"),
                f64::from_bits(hex_u64(&sim[2])),
                f64::from_bits(hex_u64(&sim[3])),
            ));
        }
    }
    reference.sort_by_key(|(t, sim, _, _)| (*t, *sim));
    assert_eq!(held.len(), reference.len());
    for ((t, sim, z_h, z_p), (rt, rsim, rz_h, rz_p)) in held.iter().zip(&reference) {
        assert_eq!((t, sim), (rt, rsim));
        assert!(close(*z_h, *rz_h) && close(*z_p, *rz_p), "sim {sim} at {t}");
    }
    let (none, untraced) = run(TraceFilter::Off);
    assert!(none.is_empty());
    assert_eq!(hashes, untraced);
}

/// The phenotype view (criteria plan, step 2): at the snapshots of (a), (b) and (b2) every
/// living person's 35 body phenotypes at age 0 are the reference's, within the census's
/// tolerance; among them founders, arrivals, pool children, town children of two pool
/// persons (`C` from the pool seed) and of town persons. A person who dies later keeps them.
#[test]
fn the_phenotype_view_is_the_references() {
    let golden: Value = serde_json::from_str(PHENOTYPES).expect("json");
    let traits: Vec<DataId> = golden["z_traits"]
        .as_array()
        .expect("traits")
        .iter()
        .map(|t| DataId::new(t.as_str().expect("id")).expect("id"))
        .collect();
    let mut kinds = BTreeSet::new();
    for (name, scenario, data) in fixtures().into_iter().take(3) {
        let scenario: Value = serde_json::from_str(scenario).expect("scenario");
        let start = start_of(&scenario, &data);
        let mut world = World::new(data);
        world.submit(Input::Start(start)).expect("accepted");
        let mut out = YearOut::default();
        let mut seen: Vec<(u64, Vec<f64>)> = Vec::new();
        for shot in golden["fixtures"][name].as_array().expect("snapshots") {
            let t = SimTime::from_secs(shot["t"].as_i64().expect("t"));
            world.advance_until(&mut out, t, &mut |_| false);
            for person in shot["persons"].as_array().expect("persons") {
                let sim = SimId::try_from(person[0].as_u64().expect("sim")).expect("id");
                kinds.insert(person[1].as_str().expect("kind").to_owned());
                let ours: Vec<f64> = traits
                    .iter()
                    .map(|t| {
                        world
                            .state()
                            .phenotype(sim, t, SimDuration::ZERO)
                            .expect("a phenotype")
                    })
                    .collect();
                for ((t, z), bits) in traits
                    .iter()
                    .zip(&ours)
                    .zip(person[2].as_array().expect("z"))
                {
                    let reference = f64::from_bits(hex_u64(bits));
                    assert!(close(*z, reference), "{name}: sim {} {t:?}", sim.get());
                }
                seen.push((sim.get(), ours));
            }
        }
        // To the end: the dead keep their phenotypes.
        let until = SimTime::from_secs(scenario["until"].as_i64().expect("until"));
        world.advance_until(&mut out, until, &mut |_| false);
        for (sim, values) in &seen {
            let sim = SimId::try_from(*sim).expect("id");
            for (t, z) in traits.iter().zip(values) {
                assert_eq!(
                    world.state().phenotype(sim, t, SimDuration::ZERO),
                    Some(*z),
                    "{name}"
                );
            }
        }
    }
    let expected: BTreeSet<String> = [
        "arrival",
        "founder",
        "pool_child",
        "town_of_town_persons",
        "town_of_two_pool_persons",
    ]
    .iter()
    .map(|k| (*k).to_owned())
    .collect();
    assert_eq!(kinds, expected);
}

/// The phenotype view on "records" (the arrival's scenario, `pool_d`, seed 6, `n_start` 30,
/// sims 26 and 31 paired by an input at 18 years) at its last year end: the reference's
/// phenotypes of every living person, among them sim 38, a town child of one arrival and
/// one town person (`C` from the world seed).
#[test]
fn the_phenotype_view_of_a_child_of_one_pool_person() {
    let golden: Value = serde_json::from_str(RECORDS_PHENOTYPES).expect("json");
    let data = pool_data("pool_d", POOL_D);
    assert_eq!(data.hash(), hex_u64(&golden["data_hash"]));
    let scenario = serde_json::json!({"start": {
        "world_seed": "0000000000000006", "founder_tag": "test:arrival",
        "world_params": {"hereditary_diseases": true, "nutrition": []},
        "founders": [], "couples": [], "pool": {"n_start": 30, "immigration": true}
    }});
    let start = start_of(&scenario, &data);
    let mut world = World::new(data);
    world.submit(Input::Start(start)).expect("accepted");
    let mut out = YearOut::default();
    let mut log = world.take_log();
    world.advance_until(&mut out, SimTime::from_secs(566_092_799), &mut |_| false);
    world
        .submit(Input::ArrangePartnership(ArrangePartnership {
            woman: SimId::try_from(26).expect("id"),
            man: SimId::try_from(31).expect("id"),
        }))
        .expect("accepted");
    let t = SimTime::from_secs(golden["t"].as_i64().expect("t"));
    world.advance_until(&mut out, t, &mut |_| false);
    log.extend(world.take_log());
    assert_eq!(
        out.year_hashes.last().map(|y| y.hash),
        Some(hex_u64(&golden["year_hash_last"]))
    );
    assert_eq!(log_hash(&log), hex_u64(&golden["log_hash"]));
    let traits: Vec<DataId> = golden["z_traits"]
        .as_array()
        .expect("traits")
        .iter()
        .map(|t| DataId::new(t.as_str().expect("id")).expect("id"))
        .collect();
    check_pure_heights(&world, "records");
    let persons = golden["persons"].as_array().expect("persons");
    assert_eq!(persons.len(), 22);
    assert!(persons.iter().any(|p| p[1] == "town_of_pool_and_town"));
    for person in persons {
        let sim = SimId::try_from(person[0].as_u64().expect("sim")).expect("id");
        for (t, bits) in traits.iter().zip(person[2].as_array().expect("z")) {
            let ours = world
                .state()
                .phenotype(sim, t, SimDuration::ZERO)
                .expect("a phenotype");
            assert!(
                close(ours, f64::from_bits(hex_u64(bits))),
                "sim {} {t:?}",
                sim.get()
            );
        }
    }
}

/// The reference's pure heights of a fixture (ADR-0015 section 4, the full height of the
/// mapping never clamped): every living person's at each snapshot that `world` has
/// reached, and at the end every archived person's at its leaving second.
fn check_pure_heights(world: &World, name: &str) {
    let golden: Value = serde_json::from_str(PURE_HEIGHTS).expect("json");
    let fixture = &golden["fixtures"][name];
    let now = world.closed_through();
    for shot in fixture["snapshots"].as_array().expect("snapshots") {
        let t = SimTime::from_secs(shot["t"].as_i64().expect("t"));
        if t != now {
            continue;
        }
        for person in shot["persons"].as_array().expect("persons") {
            let sim = SimId::try_from(person[0].as_u64().expect("sim")).expect("id");
            let ours = world.state().pure_height(sim, t).expect("a height");
            let reference = f64::from_bits(hex_u64(&person[3]));
            assert!(
                close(ours, reference),
                "{name}: sim {} at {}",
                sim.get(),
                shot["year"]
            );
        }
    }
    if fixture["archived"]["run_end"].as_i64() == Some(now.as_secs()) {
        for person in fixture["archived"]["persons"].as_array().expect("archived") {
            let sim = SimId::try_from(person[0].as_u64().expect("sim")).expect("id");
            let left = SimTime::from_secs(person[2].as_i64().expect("left"));
            let ours = world.state().pure_height(sim, left).expect("a height");
            let reference = f64::from_bits(hex_u64(&person[5]));
            assert!(close(ours, reference), "{name}: archived sim {}", sim.get());
        }
    }
}

/// The pure height (ADR-0015 section 4, criterion P6) on (a), (b) and (b2): at each snapshot
/// every living person's, at the end every archived person's, as the reference's.
#[test]
fn the_pure_height_is_the_references() {
    let golden: Value = serde_json::from_str(PURE_HEIGHTS).expect("json");
    for (name, scenario, data) in fixtures().into_iter().take(3) {
        let scenario: Value = serde_json::from_str(scenario).expect("scenario");
        let start = start_of(&scenario, &data);
        let mut world = World::new(data);
        world.submit(Input::Start(start)).expect("accepted");
        let mut out = YearOut::default();
        for shot in golden["fixtures"][name]["snapshots"]
            .as_array()
            .expect("snapshots")
        {
            let t = SimTime::from_secs(shot["t"].as_i64().expect("t"));
            world.advance_until(&mut out, t, &mut |_| false);
            check_pure_heights(&world, name);
        }
        let until = SimTime::from_secs(scenario["until"].as_i64().expect("until"));
        world.advance_until(&mut out, until, &mut |_| false);
        check_pure_heights(&world, name);
    }
}

/// The pre-run leaves MC1R and HERC2 out of its map, the reference allele of ADR-0020 section
/// 3: the census evaluates every living body there with both doses 0 (ADR-0015 section 2,
/// design note of the census 6.1), so the clamp rate of the pool run can be counted. One
/// pigment locus without the other is still no body.
#[test]
fn the_census_reads_absent_pigment_loci_as_the_reference_allele() {
    let scenario: Value = serde_json::from_str(include_str!(
        "../../../tests/golden/census/c_prerun_small.json"
    ))
    .expect("scenario");
    let first_year_end = |data: Data| {
        let start = start_of(&scenario, &data);
        let mut world = World::new(data);
        world.submit(Input::Start(start)).expect("accepted");
        let mut out = YearOut::default();
        let end = SimTime::from_secs(sim_core::time::YEAR.as_secs() - 1);
        world.advance_until(&mut out, end, &mut |_| false);
        out.census[0]
    };
    let row = first_year_end(prerun_data());
    assert!(row.living > 0);
    assert_eq!(row.body.evaluated, row.living);
    for kept in ["core:herc2", "core:mc1r"] {
        let row = first_year_end(prerun_data_with(&[kept]));
        assert!(row.living > 0, "{kept}");
        assert_eq!(row.body.evaluated, 0, "{kept}");
    }
}

/// The census's view in detail (design note of the census 6.1, review): `BodyData::new`
/// rejects the pre-run's data, `BodyData::census` takes them and reads both pigment doses as
/// 0, so a body there equals the body of the same genome on the template, where both loci
/// stand at the reference allele, in every value bit for bit. One pigment locus without the
/// other, or a present locus of another form, is no body for the census either.
#[test]
fn the_census_view_reads_absent_pigment_loci_as_dose_zero() {
    let prerun = prerun_data();
    let on_template = template_data();
    assert_eq!(BodyData::new(&prerun).err(), Some(DataError::BodyTraits));
    for kept in ["core:mc1r", "core:herc2"] {
        let data = prerun_data_with(&[kept]);
        assert_eq!(
            BodyData::census(&data).err(),
            Some(DataError::BodyTraits),
            "{kept}"
        );
    }
    for (locus, field, value) in [
        ("core:mc1r", "action", "dominant"),
        ("core:herc2", "action", "dominant"),
    ] {
        let (calibration, mut genome_map) = template();
        let entry = genome_map["loci"]
            .as_array_mut()
            .expect("loci")
            .iter_mut()
            .find(|entry| entry["id"] == locus)
            .expect("the locus");
        entry[field] = Value::from(value);
        let data = Data::new(common::definitions(calibration, genome_map).expect("definitions"))
            .expect("data");
        assert_eq!(
            BodyData::census(&data).err(),
            Some(DataError::BodyTraits),
            "{locus}"
        );
    }
    let census = BodyData::census(&prerun).expect("the census's view");
    let strict = BodyData::new(&on_template).expect("the template's body");
    let draws = Draws::new(WorldSeed::new(7));
    let own = LineageKey::founder(&DataId::new("test:census").expect("id"), 0);
    for sex in [Sex::Female, Sex::Male] {
        let genome = Genome::reference(sex);
        let inputs = |body: &BodyData<'_>| {
            BodyInputs::of_person(
                body,
                Environment::one(&draws),
                &genome,
                own,
                None,
                SimTime::START,
                sim_core::time::YEAR * 30,
                &[],
                30.0,
            )
        };
        let (ours, theirs) = (inputs(&census), inputs(&strict));
        assert_eq!((ours.mc1r, ours.herc2), (0, 0));
        assert_eq!((theirs.mc1r, theirs.herc2), (0, 0));
        let (ours, theirs) = (body_params(&census, &ours), body_params(&strict, &theirs));
        for key in BodyParamKey::ALL {
            assert_eq!(
                ours.params.get(key).to_bits(),
                theirs.params.get(key).to_bits(),
                "{sex:?} {}",
                key.as_str()
            );
        }
    }
}
