//! The domain log (ADR-0004 sections 6 and 7): pure output that the core never reads
//! back. Every entry names its causes; the payload variant name is the stable kind code.
//! New variants of `LogPayload` and `Cause` only go to the end (postcard encodes the
//! variant index; `tests/golden/log_codes.txt` pins the order).

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::data_id::Code;
use crate::genome::SexChromosome;
use crate::genome::meiosis::SegmentMeiosis;
use crate::hash::StableHasher;
use crate::ids::{EventId, InputSeq, PartnershipId, SimId};
use crate::lineage::{GenomeKey, LineageKey};
use crate::params::ParamChange;
use crate::rng::DrawKey;
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
    /// The random draw the entry follows from; seed and key give its value again
    /// (ADR-0001 section 1).
    Random(DrawRef),
}

/// The key of a random draw as a log entry names it: plain numbers, written as hex in
/// JSON. A log is output; a key read back from it addresses no draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawRef {
    #[serde(with = "crate::hex_u64")]
    pub tag: u64,
    #[serde(with = "crate::hex_u64")]
    pub subject: u64,
    #[serde(with = "crate::hex_u64")]
    pub occasion: u64,
    #[serde(with = "crate::hex_u64")]
    pub detail: u64,
}

impl From<DrawKey> for DrawRef {
    fn from(key: DrawKey) -> DrawRef {
        DrawRef {
            tag: key.tag.get(),
            subject: key.subject.get(),
            occasion: key.occasion,
            detail: key.detail,
        }
    }
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
    /// A founder entered the world (creation entry of a SimId). The lineage key names
    /// the same founder in every fork; the ID does not (ADR-0001 section 6).
    #[serde(rename = "core:founder_added")]
    FounderAdded {
        born: SimTime,
        lineage_key: LineageKey,
    },
    /// Dummy event of step 2: a yearly birthday.
    #[serde(rename = "core:birthday")]
    Birthday { age_years: u32 },
    /// An override block changed a world parameter (ADR-0001 section 6).
    #[serde(rename = "core:param_changed")]
    ParamChanged { change: ParamChange },
    /// A sim died of this cause and left for the archive (departure entry of a SimId).
    #[serde(rename = "core:death")]
    Death { cause: Code },
    /// A woman and a man (the subjects, in this order) formed a partnership (creation
    /// entry of a PartnershipId).
    #[serde(rename = "core:partnered")]
    Partnered { partnership: PartnershipId },
    /// A woman and a man (the subjects) conceived the child with this lineage key.
    #[serde(rename = "core:conception")]
    Conception { lineage_key: LineageKey },
    /// One meiosis of a conception (ADR-0004 section 7, genetics specification 2.3);
    /// subject the parent. Per segment the start strand and the crossover positions, and
    /// the sex chromosome passed on (always X from the mother).
    #[serde(rename = "core:meiosis")]
    Meiosis {
        genome_key: GenomeKey,
        segments: Vec<SegmentMeiosis>,
        sex_chromosome: SexChromosome,
    },
    /// The child with this lineage key was born (creation entry of a SimId; subjects the
    /// child, the mother and the father).
    #[serde(rename = "core:birth")]
    Birth { lineage_key: LineageKey },
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
        for entry in entries {
            let bytes =
                postcard::to_allocvec(entry).unwrap_or_else(|_| panic!("invariant log.encode"));
            self.0.update(&bytes);
        }
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
