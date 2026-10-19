//! The criteria town (criteria plan step 5, ADR-0020 section 8): P3, P4 city, P5, P9 and F11
//! on a town founded from the pool, `n_start` 10 000, capacity 10 000, immigration on, 200
//! years, the hereditary diseases on, one gate seed. Too large for the gate, so it runs as a
//! tool in release, alone on the machine:
//!
//! `cargo run --release --locked -p xtask -- criteria town --map <dir> --pool <pool.bin>
//! --out <dir>`
//!
//! It runs the town and the same town without capacity (so without emigration) on one data,
//! then writes `export.ndjson` and `report.json` into `<dir>` and `<dir>/open`; it is red
//! when a check is red or a negative twin is green: `h²_T` is decided in the open town (Grok
//! GH-1: emigration tilts the lifespans of those who die in the town), every other check in
//! the criteria town.

use std::path::{Path, PathBuf};

use serde_json::Value;
use sim_core::data::Data;
use sim_core::data_id::DataId;
use sim_core::input::{Input, PoolStart, Start};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;
use sim_core::world::World;

use super::report::report;
use super::write::{Observer, write};

type Result<T> = std::result::Result<T, String>;

/// Gate seed of the criteria town (protected list; never reseeded).
pub const SEED_TOWN: u64 = 0x5eed_0004_0000_0001;
/// The town of ADR-0020 section 8: its start, its capacity and its years.
const N_START: u64 = 10_000;
const CAPACITY: u64 = 10_000;
const YEARS: i64 = 200;
const YEAR: i64 = 31_449_600;

/// The data of a town: the repository's life course data, the calibration and map of
/// `map_dir`, the pool's bytes, and the pool's frequencies, taken from the calibration's
/// `pool_frequencies` entry (they travel with the pool and enter the data hash; the pool was
/// extracted on the definitions without them).
pub fn town_data(root: &Path, map_dir: &Path, pool: &[u8]) -> Result<Data> {
    let demography = crate::map4::read(root, "data/calibration/demography.json")?;
    let (calibration, map) = crate::prerun::read_files(map_dir)?;
    let (calibration, frequencies) = crate::pool_file::take_frequencies(&calibration, &map)?;
    let definitions = crate::prerun::definitions_of(&demography, &calibration, &map)?;
    let frequencies = crate::pool_file::finite(frequencies)?;
    Data::with_pool(definitions, pool, frequencies).map_err(|e| format!("the town's data: {e}"))
}

/// How a town starts and how long it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub seed: u64,
    pub tag: &'static str,
    pub n_start: u64,
    pub capacity: Option<u64>,
    pub immigration: bool,
    /// A village (G4): persons of each sex, each alone, instead of the draw's households.
    pub village: bool,
    /// The last second of the run, inclusive.
    pub end: SimTime,
}

/// The criteria town's plan.
pub fn criteria_town() -> Plan {
    Plan {
        seed: SEED_TOWN,
        tag: "core:criteria_town",
        n_start: N_START,
        capacity: Some(CAPACITY),
        immigration: true,
        village: false,
        end: SimTime::from_secs(YEARS * YEAR - 1),
    }
}

/// The same town without capacity, so without emigration, where `h²_T` is measured (Grok
/// GH-1): every town-born person lives and dies in the town.
pub fn open_town() -> Plan {
    Plan {
        capacity: None,
        ..criteria_town()
    }
}

/// The checks decided in the open town (Grok GH-1).
pub fn decided_open(check: &str) -> bool {
    check.starts_with("f11_h2t_")
}

/// The checks decided in the criteria town: all others.
pub fn decided_closed(check: &str) -> bool {
    !decided_open(check)
}

/// A pool town founded from the households of the draw for occasion 0 until they bring
/// `n_start` persons (a village: `n_start / 2` persons of each sex, each alone), run to its
/// end with the hereditary diseases on, its log fed to the export's observer.
pub fn run_town(data: Data, plan: &Plan) -> Result<(World, Observer)> {
    let pool = if plan.village {
        PoolStart::village(&data, plan.seed, plan.n_start)
            .ok_or("no village: the data hold no pool, n_start is odd or too large")?
    } else {
        PoolStart::drawn(&data, plan.seed, plan.n_start, plan.immigration)
            .ok_or("the data hold no pool")?
    };
    let start = Start {
        world_seed: plan.seed,
        founder_tag: DataId::new(plan.tag).map_err(|e| format!("{}: {e:?}", plan.tag))?,
        world_params: WorldParams {
            hereditary_diseases: true,
            nutrition: Vec::new(),
        },
        founders: Vec::new(),
        couples: Vec::new(),
        capacity: plan.capacity,
        pool: Some(pool),
    };
    let mut observer = Observer::default();
    let (world, _) =
        crate::prerun::run_observed(data, Input::Start(start), plan.end, &mut |entries| {
            observer.observe(entries)
        })?;
    Ok((world, observer))
}

/// What makes a report red (spec 8.1): every check whose `pass` is false, and every negative
/// twin whose `pass` is true (a twin must be red). P4 city counts as one check over its
/// decades; reported-only checks (`pass` null) count for neither. Only the checks for which
/// `taken` holds count.
pub fn reds(report: &Value, taken: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut reds = Vec::new();
    for check in report["checks"].as_array().into_iter().flatten() {
        let name = check["check"].as_str().unwrap_or("?");
        if !taken(name) {
            continue;
        }
        if check["pass"] == Value::Bool(false) {
            let finding = check["finding"].as_str().unwrap_or("red");
            reds.push(format!("{name}: {finding}"));
        }
        let twin_green =
            check["twin"]["pass"] == Value::Bool(true) || check["twin_pass"] == Value::Bool(true);
        if twin_green {
            reds.push(format!("{name}: the negative twin is green"));
        }
    }
    reds
}

const USAGE: &str = "criteria town --map <dir> --pool <pool.bin> --out <dir>";

/// `cargo xtask criteria town --map <dir> --pool <pool.bin> --out <dir>`.
pub fn command(root: &Path, args: &[String]) -> Result<()> {
    let value = |name: &str| -> Result<&str> {
        let at = args.iter().position(|arg| arg == name).ok_or(USAGE)?;
        args.get(at + 1)
            .map(String::as_str)
            .ok_or_else(|| USAGE.to_owned())
    };
    if args.len() != 6 {
        return Err(USAGE.into());
    }
    let (map, pool, out) = (value("--map")?, value("--pool")?, value("--out")?);
    if cfg!(debug_assertions) {
        // The result is the same in both profiles (the gate runs both); only the time is not.
        eprintln!(
            "criteria town: a debug build takes many times longer; use cargo run --release --locked -p xtask -- criteria town ..."
        );
    }
    let bytes = std::fs::read(pool).map_err(|e| format!("{pool}: {e}"))?;
    let data = town_data(root, Path::new(map), &bytes)?;
    let out = Path::new(out);
    let runs: [Run; 2] = [
        (criteria_town(), out.to_path_buf(), decided_closed),
        (open_town(), out.join("open"), decided_open),
    ];
    let all = towns(&data, &runs)?;
    if all.is_empty() {
        Ok(())
    } else {
        Err(format!("red:\n  {}", all.join("\n  ")))
    }
}

/// A town to run: its plan, its directory and the checks it decides.
pub type Run = (Plan, PathBuf, fn(&str) -> bool);

/// Runs every town on the same data, then writes each one's `export.ndjson` and
/// `report.json` into its directory: nothing is written unless every town ran, so a failed
/// open town never leaves a fresh criteria town beside stale files (review). Returns the reds
/// of the checks each town decides.
pub fn towns(data: &Data, runs: &[Run]) -> Result<Vec<String>> {
    let mut ran = Vec::new();
    for (plan, dir, taken) in runs {
        let (world, observer) = run_town(data.clone(), plan)?;
        let text = write(&world, &observer, plan.end)?;
        let report = report(&text)?;
        ran.push((dir, text, report, taken));
    }
    let mut all = Vec::new();
    for (dir, text, report, taken) in ran {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(dir.join("export.ndjson"), &text).map_err(|e| e.to_string())?;
        let mut json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        json.push('\n');
        std::fs::write(dir.join("report.json"), json).map_err(|e| e.to_string())?;
        println!("criteria town: {}", dir.display());
        all.extend(reds(&report, taken));
    }
    Ok(all)
}

#[cfg(test)]
#[path = "town_tests.rs"]
mod tests;
