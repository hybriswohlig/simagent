//! The files of a run directory (ADR-0004 sections 4 and 8, B17): JSON through serde,
//! one object per line in the `.ndjson` files, u64 values with full range as hex.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sim_core::canonical::YearHash;
use sim_core::data_id::DataId;
use sim_core::input::{Couple, Founder, Start};
use sim_core::params::WorldParams;
use sim_core::time::SimTime;

use crate::error::CliError;

pub const MANIFEST: &str = "manifest.json";
pub const SCENARIO: &str = "scenario.resolved.json";
pub const INPUT_LOG: &str = "input.ndjson";
pub const EVENTS: &str = "events.ndjson";
pub const CENSUS: &str = "census.csv";
pub const SAVE: &str = "state.save";
/// Written by `simagent export`, not part of a run.
pub const PEDIGREE: &str = "pedigree.json";

/// The format version of the files this program writes and reads: 5 since founders draw the
/// alleles they do not name and a scenario names its pool (design note of the map-4 switch,
/// section 3).
pub const FORMAT_VERSION: u32 = 5;

/// A scenario file: the `Start` before its pool is drawn, and how far to run. The CLI
/// resolves it into the `Start` of the core (`scenario.resolved.json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFile {
    pub format_version: u32,
    pub start: StartFile,
    pub until: SimTime,
}

/// The `start` of a scenario file: the fields of `Start`, the pool as its choice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartFile {
    #[serde(with = "sim_core::hex_u64")]
    pub world_seed: u64,
    pub founder_tag: DataId,
    pub world_params: WorldParams,
    pub founders: Vec<Founder>,
    pub couples: Vec<Couple>,
    /// Required (`null` for none), as in `Start`.
    #[serde(deserialize_with = "Option::deserialize")]
    pub capacity: Option<u64>,
    /// Required (`null` for a town of the founders above).
    #[serde(deserialize_with = "Option::deserialize")]
    pub pool: Option<PoolChoice>,
}

/// A town from the data's pool: its size at the start and whether households come yearly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolChoice {
    pub n_start: u64,
    pub immigration: bool,
}

/// The resolved scenario of step 2: the `Start` input and how far to run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub format_version: u32,
    pub start: Start,
    pub until: SimTime,
}

/// First line of `input.ndjson` (ADR-0004 section 4). `build_commit` is informative.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogHeader {
    pub format_version: u32,
    pub core_version: u32,
    #[serde(with = "sim_core::hex_u64")]
    pub data_hash: u64,
    pub build_commit: String,
}

/// Last line of `input.ndjson` after an orderly end: the clock at that moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogEnd {
    pub end: SimTime,
}

/// `manifest.json` (ADR-0004 section 8): what a replay or a fork compares against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub core_version: u32,
    #[serde(with = "sim_core::hex_u64")]
    pub data_hash: u64,
    pub build_commit: String,
    #[serde(with = "sim_core::hex_u64")]
    pub world_seed: u64,
    pub closed_through: SimTime,
    pub year_hashes: Vec<YearHash>,
    #[serde(with = "sim_core::hex_u64")]
    pub log_hash: u64,
    /// Only in the manifest of a fork.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork: Option<ForkInfo>,
}

/// The block `fork` of a manifest (ADR-0004 section 8, ADR-0001 section 6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkInfo {
    /// The parent run, named by the hash of its log: a path would differ per machine.
    #[serde(with = "sim_core::hex_u64")]
    pub parent_log_hash: u64,
    /// The fork point is the start of this year.
    pub from_year: u32,
    /// Inputs of the parent that would take effect from the fork point on.
    pub dropped_inputs: u64,
}

/// The JSON form of `value` on one line.
pub fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| panic!("invariant cli.json_encode"))
}

/// The stable code of a core enum that serializes as a plain string.
pub fn code<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(name)) => name,
        _ => panic!("invariant cli.code_is_a_string"),
    }
}

/// A manifest of this program's format version; anything else is `manifest_invalid`.
pub fn read_manifest(text: &str) -> Result<Manifest, CliError> {
    let manifest: Manifest = parse_strict(text.trim_end())
        .ok_or_else(|| CliError::new("manifest_invalid", "not a manifest"))?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(CliError::new(
            "manifest_invalid",
            "not a manifest of format version 5",
        ));
    }
    Ok(manifest)
}

/// Reads `text` as `T` and accepts it only if `T` writes the same JSON value back.
/// serde also reads a struct from a list of values without field names; that form
/// and anything else `T` would not write is rejected here (ADR-0004 section 4).
pub fn parse_strict<T: Serialize + DeserializeOwned>(text: &str) -> Option<T> {
    let value: T = serde_json::from_str(text).ok()?;
    let given: Value = serde_json::from_str(text).ok()?;
    (serde_json::to_value(&value).ok()? == given).then_some(value)
}

pub fn read(path: &Path) -> Result<Vec<u8>, CliError> {
    fs::read(path).map_err(|error| CliError::io(path, &error))
}

pub fn read_text(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|error| CliError::io(path, &error))
}

pub fn write(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    fs::write(path, bytes).map_err(|error| CliError::io(path, &error))
}

/// Writes `bytes` to a fresh file next to `path` and renames it over `path`. A link at
/// `path` is replaced, never followed: the export writes into a directory it did not
/// create.
pub fn replace(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    let fresh = Path::new(&name);
    // The leftover of an interrupted export may itself be a link; it goes first.
    if fs::symlink_metadata(fresh).is_ok() {
        fs::remove_file(fresh).map_err(|error| CliError::io(fresh, &error))?;
    }
    fs::write(fresh, bytes).map_err(|error| CliError::io(fresh, &error))?;
    fs::rename(fresh, path).map_err(|error| CliError::io(path, &error))
}

pub fn append(path: &Path, text: &str) -> Result<(), CliError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|error| CliError::io(path, &error))
}
