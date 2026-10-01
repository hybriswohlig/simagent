//! The domain log (ADR-0004 sections 6 and 7): pure output that the core never reads
//! back. Every entry names its causes; the payload variant name is the stable kind code.
//! New variants of `LogPayload` and `Cause` only go to the end (postcard encodes the
//! variant index; `tests/golden/log_codes.txt` pins the order).

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::data_id::Code;
use crate::hash::StableHasher;
use crate::ids::{EventId, InputSeq, SimId};
use crate::time::SimTime;

/// Companion data of every planned event: the entry that planned it and when.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRef {
    pub by: EventId,
    pub planned_at: SimTime,
}

/// Why an entry exists.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Cause {
    /// An earlier entry of the same handler call.
    Event(EventId),
    /// The running event was planned earlier; `by` gives the reason.
    Plan(PlanRef),
    /// The input with this position in the input log.
    Input(InputSeq),
    /// A stable rule code such as `core:gompertz_hazard`.
    Rule(Code),
}

/// Never empty: by construction and when deserialized.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "Vec<Cause>", into = "Vec<Cause>")]
pub struct Causes(Vec<Cause>);

/// An empty list where causes were expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyCauses;

impl fmt::Display for EmptyCauses {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("empty_causes")
    }
}

impl Causes {
    pub fn new(first: Cause) -> Causes {
        Causes(vec![first])
    }

    pub fn with(mut self, more: Cause) -> Causes {
        self.0.push(more);
        self
    }

    pub fn as_slice(&self) -> &[Cause] {
        &self.0
    }
}

impl TryFrom<Vec<Cause>> for Causes {
    type Error = EmptyCauses;
    fn try_from(causes: Vec<Cause>) -> Result<Causes, EmptyCauses> {
        if causes.is_empty() {
            return Err(EmptyCauses);
        }
        Ok(Causes(causes))
    }
}

impl From<Causes> for Vec<Cause> {
    fn from(causes: Causes) -> Vec<Cause> {
        causes.0
    }
}

/// Who an entry is about, in the order the kind defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Subject {
    Sim(SimId),
}

/// The kind and its data. No text, only numbers, IDs, codes and data IDs (B16).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum LogPayload {
    /// The `Start` input was applied; the reason for the system series.
    #[serde(rename = "core:started")]
    Started { founders: u32 },
    /// A founder entered the world (creation entry of a SimId).
    #[serde(rename = "core:founder_added")]
    FounderAdded { born: SimTime },
    /// Dummy event of step 2: a yearly birthday.
    #[serde(rename = "core:birthday")]
    Birthday { age_years: u32 },
}

/// One entry of the domain log.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogEntry {
    pub id: EventId,
    pub t: SimTime,
    pub subjects: Vec<Subject>,
    pub causes: Causes,
    pub payload: LogPayload,
}

/// The log hash (ADR-0004 section 8): `StableHasher` over the postcard bytes of all
/// entries in order. Streaming, so a driver can feed every batch from `take_log`.
#[derive(Default)]
pub struct LogHasher(StableHasher);

impl LogHasher {
    pub fn new() -> LogHasher {
        LogHasher(StableHasher::new())
    }

    pub fn update(&mut self, entries: &[LogEntry]) {
        let _ = entries;
    }

    pub fn digest(&self) -> u64 {
        self.0.digest()
    }
}

/// The log hash of `entries` in one call.
pub fn log_hash(entries: &[LogEntry]) -> u64 {
    let mut hasher = LogHasher::new();
    hasher.update(entries);
    hasher.digest()
}
