//! The files of a run directory (ADR-0004 sections 4 and 8, B17): JSON through serde,
//! one object per line in the `.ndjson` files, u64 values with full range as hex.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sim_core::canonical::YearHash;
use sim_core::input::Start;
use sim_core::time::SimTime;

use crate::error::CliError;

pub const MANIFEST: &str = "manifest.json";
pub const SCENARIO: &str = "scenario.resolved.json";
pub const INPUT_LOG: &str = "input.ndjson";
pub const EVENTS: &str = "events.ndjson";
pub const CENSUS: &str = "census.csv";
pub const SAVE: &str = "state.save";

/// The format version of the files this program writes and reads.
pub const FORMAT_VERSION: u32 = 1;

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

pub fn append(path: &Path, text: &str) -> Result<(), CliError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|error| CliError::io(path, &error))
}
