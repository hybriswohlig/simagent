//! The villages of P4 (ADR-0020 sections 8 and 11, G4, G5, K2 and Grok D1 to D3; spec 8.2
//! P4): 30 villages founded from the repository's pool, each of 40 women and 40 men who
//! arrive alone, capacity 160, no immigration, the hereditary diseases on. Each runs to 1200
//! years, is measured with the kit (its last block of 30 birth years whose youngest turn 18
//! by the end) and dropped; the first 20 in list order with a block of two units or more
//! count. While the power against "no drift" stays below 0.9, all 30 run again to an end 300
//! years later, up to 2100 (the same runs, ADR-0001). Too large for the gate, so it runs as
//! a tool in release, one village at a time:
//!
//! `cargo run --release --locked -p xtask -- criteria village --out <dir>`
//!
//! It writes `village.json` (each end taken: every village's block and whether it counts,
//! then the check over the counted and its twin) and the exports of the last end into
//! `<dir>/<i>/export.ndjson`. It is red when fewer than 20 villages count, when the check has
//! a finding or decides red, and when its twin passes.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::hash::stable_hash64;
use sim_core::time::SimTime;

use super::export::read;
use super::report::{VILLAGE_H2, bits, village as block_of};
use super::stats::{
    VILLAGE_SEEDS, Village, VillageFinding, variance, village, village_comparison,
    village_expectation,
};
use super::town::{Plan, run_town};
use super::units::{Town, YEAR};
use super::write::write;

type Result<T> = std::result::Result<T, String>;

/// Gate seed of the first village (protected list; never reseeded): village `i` has
/// `SEED_VILLAGE + i`, `i` below `VILLAGES`. The first two are the seeds of the core's
/// village tests.
pub const SEED_VILLAGE: u64 = 0x5eed_0004_0000_0002;
/// The villages of the list (Grok D1); the first `VILLAGE_SEEDS` with a block count.
pub const VILLAGES: usize = 30;
/// The village of ADR-0020 section 8 (G4), with the capacity of Grok D1.
const N_START: u64 = 80;
const CAPACITY: u64 = 160;
/// The ends in years: 1200, then in steps of 300 up to 2100 (spec 8.2 P4).
pub const ENDS: [i64; 4] = [1200, 1500, 1800, 2100];
/// The power against "no drift" a decision needs (spec 8.2 P4).
const POWER: f64 = 0.9;

/// The seeds of the 30 villages, in list order.
pub fn seeds() -> Vec<u64> {
    (0..VILLAGES)
        .map(|i| {
            SEED_VILLAGE + u64::try_from(i).unwrap_or_else(|_| panic!("invariant village.seed"))
        })
        .collect()
}

/// Village `seed` run to the end of year `years − 1`.
pub fn village_plan(seed: u64, years: i64) -> Plan {
    Plan {
        seed,
        tag: "core:criteria_village",
        n_start: N_START,
        capacity: Some(CAPACITY),
        immigration: false,
        village: true,
        end: SimTime::from_secs(years * YEAR - 1),
    }
}

/// The repository's data: the documents under `data/`, the pool and its frequencies, as the
/// CLI builds its own.
pub fn repository_data(root: &Path) -> Result<Data> {
    let demography = crate::map4::read(root, "data/calibration/demography.json")?;
    let (calibration, map) = crate::prerun::read_files(&root.join("data"))?;
    let definitions = crate::prerun::definitions_of(&demography, &calibration, &map)?;
    let path = root.join("data/pool/pool.bin");
    let pool = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let path = root.join("data/pool/pool_frequencies.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let entries = file["pool_frequencies"]
        .as_object()
        .ok_or("pool_frequencies: no object")?;
    let mut frequencies = BTreeMap::new();
    for (id, entry) in entries {
        if entry["unit"] != "fraction" {
            return Err(format!("pool_frequencies.{id}: not a fraction"));
        }
        let q = entry["value"]
            .as_f64()
            .ok_or_else(|| format!("pool_frequencies.{id}: no value"))?;
        frequencies.insert(DataId::new(id).map_err(|e| format!("{id}: {e:?}"))?, q);
    }
    let frequencies = crate::pool_file::finite(frequencies)?;
    let shocks = shocks(root)?;
    Data::with_pool(definitions, &pool, frequencies)
        .and_then(|data| data.with_shocks(shocks))
        .map_err(|e| format!("the repository's data: {e}"))
}

/// The shock templates of `data/shocks.json` without their text, as sim-cli reads them
/// (ADR-0023 section 3); the core checks the values.
fn shocks(root: &Path) -> Result<Vec<sim_core::data::ShockTemplate>> {
    let path = root.join("data/shocks.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    // The fields of the file and of each template, exactly (`deny_unknown_fields`, as
    // sim-cli reads them). A key given twice keeps its last value here; sim-cli refuses
    // it, and its test `the_base_game_loads` reads this same file in the gate.
    let exactly = |value: &Value, keys: &[&str]| {
        value.as_object().is_some_and(|object| {
            object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
        })
    };
    if !exactly(&file, &["format", "version", "shocks"])
        || file["format"] != "simagent.shocks"
        || file["version"] != 1
    {
        return Err(format!("{}: not simagent.shocks version 1", path.display()));
    }
    let entries = file["shocks"].as_array().ok_or("shocks: no list")?;
    entries
        .iter()
        .map(|entry| {
            let field = |name: &str| format!("shocks: an entry without its {name}");
            if !exactly(entry, &["id", "kind", "years", "index", "source"]) {
                return Err(field("fields id, kind, years, index and source alone"));
            }
            if entry["kind"] != "hunger" || !entry["source"].is_string() {
                return Err(field("kind hunger and its source"));
            }
            let id = entry["id"].as_str().ok_or_else(|| field("id"))?;
            let years = entry["years"].as_u64().ok_or_else(|| field("years"))?;
            let index = entry["index"].as_f64().ok_or_else(|| field("index"))?;
            Ok(sim_core::data::ShockTemplate {
                id: DataId::new(id).map_err(|e| format!("{id}: {e:?}"))?,
                kind: sim_core::data::ShockKind::Hunger,
                years: u32::try_from(years).map_err(|_| field("years"))?,
                index: sim_core::finite::FiniteF64::try_new(index).map_err(|_| field("index"))?,
            })
        })
        .collect()
}

/// One village at one end: its block as the report writes it (with the export's hash), and
/// `(V, E)` if the block counts (no finding, or nobody living at the end: ADR-0020
/// section 11).
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub block: Value,
    pub counted: Option<(f64, f64)>,
}

/// Whether a block with the report's `finding` counts (Grok D1, D1b): every block with at
/// least two units, also one whose village has nobody alive at the end (the report still
/// says so); not a missing block nor one with fewer than two units.
pub fn counts(finding: Option<&str>) -> bool {
    matches!(finding, None | Some("no_living_person"))
}

/// Measures a village from the text of its export.
pub fn measure(text: &str) -> Result<Measured> {
    let export = read(text)?;
    let town = Town::new(&export)?;
    let years = (export.header.end + 1) / YEAR;
    let found = town.village();
    let mut block = block_of(years, found.as_ref());
    block["export"] = json!(format!("{:016x}", stable_hash64(text.as_bytes())));
    let counted = match (&found, counts(block["finding"].as_str())) {
        (Some(found), true) => {
            let v = variance(&found.units.values).ok_or("invariant village.variance")?;
            let pairs: Vec<(u64, u64)> = (found.blocks.iter()).map(|(_, m, f)| (*m, *f)).collect();
            Some((v, village_expectation(VILLAGE_H2, &pairs)))
        }
        _ => None,
    };
    Ok(Measured { block, counted })
}

/// What the check over the seeds decides at an end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Pass,
    Red,
    Extend,
}

/// Below the power of D1 a check extends or, at the last end, is red.
pub fn underpowered(check: &Village) -> bool {
    check.power < POWER
}

/// The kit's decision at an end of `years` (K2, the reference's `village_aggregate`): below
/// the power the runs extend unless this is the last end, where the check is red
/// (underpowered); otherwise the comparison decides.
pub fn decide(check: &Village, years: i64) -> Decision {
    if underpowered(check) {
        if years < ENDS[ENDS.len() - 1] {
            Decision::Extend
        } else {
            Decision::Red
        }
    } else if check.pass {
        Decision::Pass
    } else {
        Decision::Red
    }
}

/// The twin (Grok D3): the measured values shifted to mean 1 with the same spread, the "no
/// drift" model the power assumes, against the same expectations. It must be red; it is
/// green when its comparison passes, whatever its own power.
pub fn twin(v: &[f64], e: &[f64], mean_v: f64) -> std::result::Result<Village, VillageFinding> {
    let shifted: Vec<f64> = v.iter().map(|v| v - mean_v + 1.0).collect();
    village_comparison(&shifted, e)
}

fn finding_name(finding: VillageFinding) -> &'static str {
    match finding {
        VillageFinding::BelowMinimum => "below_minimum",
        VillageFinding::NoVariance => "no_variance",
        VillageFinding::NoSpread => "no_spread",
    }
}

fn check_json(check: &Village, decision: Option<Decision>) -> Value {
    json!({
        "v_bar": bits(check.mean_v), "e_bar": bits(check.mean_e), "s": bits(check.s),
        "se": bits(check.se), "tol": bits(4.0 * check.se), "power": bits(check.power),
        "pass": check.pass,
        "decision": decision.map(|d| format!("{d:?}").to_lowercase()),
    })
}

/// One end judged: its reds (final unless it extends), its decision and its record.
#[derive(Clone, Debug, PartialEq)]
pub struct Judged {
    pub reds: Vec<String>,
    pub decision: Decision,
    pub json: Value,
}

/// The villages that count at an end (Grok D1): in list order the first `VILLAGE_SEEDS`
/// whose block counts (no finding, or nobody living at the end: ADR-0020 section 11).
pub fn counted(measured: &[Measured]) -> Vec<usize> {
    (measured.iter().enumerate())
        .filter(|(_, m)| m.counted.is_some())
        .map(|(i, _)| i)
        .take(VILLAGE_SEEDS)
        .collect()
}

/// Judges the villages of one end: fewer than 20 that count is red at once and nothing
/// extends (Grok D1); then the check over the counted and, unless it extends, its twin.
pub fn judge(years: i64, measured: &[Measured]) -> Judged {
    let taken = counted(measured);
    let blocks: Vec<Value> = (measured.iter().enumerate())
        .map(|(i, m)| {
            let mut block = m.block.clone();
            block["counted"] = json!(taken.contains(&i));
            block
        })
        .collect();
    let record = |check: Value, twin: Value| json!({"end_years": years, "villages": blocks, "check": check, "twin": twin});
    let mut reds = Vec::new();
    if taken.len() < VILLAGE_SEEDS {
        reds.push(format!(
            "village_check: fewer_than_20_villages ({})",
            taken.len()
        ));
        return Judged {
            reds,
            decision: Decision::Red,
            json: record(Value::Null, Value::Null),
        };
    }
    let (v, e): (Vec<f64>, Vec<f64>) = (taken.iter()).filter_map(|i| measured[*i].counted).unzip();
    let check = match village(&v, &e) {
        Ok(check) => check,
        Err(finding) => {
            let name = finding_name(finding);
            reds.push(format!("village_check: {name}"));
            return Judged {
                reds,
                decision: Decision::Red,
                json: record(json!({"finding": name}), Value::Null),
            };
        }
    };
    let decision = decide(&check, years);
    if decision == Decision::Extend {
        return Judged {
            reds,
            decision,
            json: record(check_json(&check, Some(decision)), Value::Null),
        };
    }
    if decision == Decision::Red {
        let why = if underpowered(&check) {
            "underpowered"
        } else {
            "outside"
        };
        reds.push(format!("village_check: {why}"));
    }
    let twin_json = match twin(&v, &e, check.mean_v) {
        Ok(twin) => {
            if twin.pass {
                reds.push("village_check: the negative twin is green".to_owned());
            }
            check_json(&twin, None)
        }
        Err(finding) => {
            let name = finding_name(finding);
            reds.push(format!("village_check: the twin has {name}"));
            json!({"finding": name})
        }
    };
    Judged {
        reds,
        decision,
        json: record(check_json(&check, Some(decision)), twin_json),
    }
}

/// Runs the 30 villages to each end until one decides, writes `village.json` and the last
/// end's exports into `out`, and returns the reds.
pub fn villages(data: &Data, out: &Path) -> Result<Vec<String>> {
    let mut ends = Vec::new();
    let mut texts = Vec::new();
    let mut reds = Vec::new();
    for years in ENDS {
        let mut measured = Vec::new();
        texts.clear();
        for (i, seed) in seeds().into_iter().enumerate() {
            let plan = village_plan(seed, years);
            let (world, observer) = run_town(data.clone(), &plan)?;
            let text = write(&world, &observer, plan.end)?;
            let one = measure(&text)?;
            println!(
                "criteria village: {years} years, village {i}: n {}, living {}",
                one.block["n"], one.block["living"]
            );
            measured.push(one);
            texts.push(text);
        }
        let judged = judge(years, &measured);
        ends.push(judged.json);
        if judged.decision != Decision::Extend {
            reds = judged.reds;
            break;
        }
    }
    for (i, text) in texts.iter().enumerate() {
        let dir = out.join(i.to_string());
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(dir.join("export.ndjson"), text).map_err(|e| e.to_string())?;
    }
    let document = json!({
        "format": "simagent.criteria.village",
        "version": 1,
        "seeds": seeds().iter().map(|s| format!("{s:016x}")).collect::<Vec<_>>(),
        "ends": ends,
        "reds": reds,
    });
    let mut text = serde_json::to_string_pretty(&document).map_err(|e| e.to_string())?;
    text.push('\n');
    std::fs::write(out.join("village.json"), text).map_err(|e| e.to_string())?;
    println!("criteria village: {}", out.display());
    Ok(reds)
}

const USAGE: &str = "criteria village --out <dir>";

/// `cargo xtask criteria village --out <dir>`.
pub fn command(root: &Path, args: &[String]) -> Result<()> {
    let [flag, out] = args else {
        return Err(USAGE.into());
    };
    if flag != "--out" {
        return Err(USAGE.into());
    }
    if cfg!(debug_assertions) {
        // The result is the same in both profiles (the gate runs both); only the time is not.
        eprintln!(
            "criteria village: a debug build takes many times longer; use cargo run --release --locked -p xtask -- criteria village ..."
        );
    }
    let data = repository_data(root)?;
    let out = Path::new(out);
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let reds = villages(&data, out)?;
    if reds.is_empty() {
        Ok(())
    } else {
        Err(format!("red:\n  {}", reds.join("\n  ")))
    }
}

#[cfg(test)]
#[path = "village_tests.rs"]
mod tests;
