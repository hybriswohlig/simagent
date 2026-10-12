//! Inputs (ADR-0004 section 4): the only way from outside into the core, applied by
//! `World::submit` at event boundaries and recorded in the input log. The JSON form is
//! the command schema (B15): externally tagged, snake_case, unknown fields rejected.
//! Variants arrive with their first handler and only go to the end.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;
use crate::genome::{LocusAlleles, Sex};
use crate::ids::{InputSeq, SimId};
use crate::params::{ParamChange, WorldParams};
use crate::time::SimTime;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    /// The resolved scenario; only as the first input.
    Start(Start),
    /// The override block of a fork (ADR-0001 section 6). `simagent fork` submits it
    /// stamped `T_X - 1 s`, so it acts before the first event of year X.
    ForkOverride(ForkOverride),
    /// Pairs two living sims ("Paare stiften", A2).
    ArrangePartnership(ArrangePartnership),
}

/// Further parts of the scenario (mod list, shocks, stop condition; ADR-0004 section 4)
/// arrive as fields with their first reader.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    #[serde(with = "crate::hex_u64")]
    pub world_seed: u64,
    /// Data ID of the scenario; founder keys use its `str_key` (ADR-0001).
    pub founder_tag: DataId,
    pub world_params: WorldParams,
    /// The causes of death come from the data (ADR-0017 section 2), not from the scenario.
    pub founders: Vec<Founder>,
    /// The fixed couples of founders (ADR-0001, Durchstich), paired after all founders
    /// entered the world, in list order. When they conceive comes from the data
    /// (ADR-0017 section 4), not from the scenario.
    pub couples: Vec<Couple>,
    /// The soft capacity `K` (ADR-0020 section 6): above it households emigrate at the end
    /// of each year; `None` means no capacity. Required in JSON (`null` for none):
    /// serde would otherwise take a missing field as `None`.
    #[serde(deserialize_with = "Option::deserialize")]
    pub capacity: Option<u64>,
}

/// A couple of founders, by their places in the list of founders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Couple {
    pub woman: u32,
    pub man: u32,
}

/// A woman and a man to pair; both must live and be in no partnership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrangePartnership {
    pub woman: SimId,
    pub man: SimId,
}

/// World parameters with their new values, applied in list order. A block is never
/// empty and names no parameter twice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkOverride {
    pub changes: Vec<ParamChange>,
}

impl ForkOverride {
    /// Why this block is rejected whatever the state of the world, if at all: it has no
    /// change, it names a parameter twice (a year of the nutrition index is a parameter),
    /// or it sets a nutrition index out of range (ADR-0013).
    pub fn rejection(&self) -> Option<RejectCode> {
        // An empty block would take a place in the input log and make a fork without
        // override differ from its parent.
        if self.changes.is_empty() {
            return Some(RejectCode::EmptyOverride);
        }
        let twice = self.changes.iter().enumerate().any(|(index, change)| {
            self.changes[..index]
                .iter()
                .any(|earlier| earlier.same_param(*change))
        });
        if twice {
            return Some(RejectCode::OverrideParamTwice);
        }
        let out_of_range = self.changes.iter().any(|change| match change {
            ParamChange::NutritionYear(entry) => !entry.in_range(),
            ParamChange::HereditaryDiseases(_) => false,
        });
        out_of_range.then_some(RejectCode::NutritionOutOfRange)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Founder {
    pub born: SimTime,
    pub sex: Sex,
    /// The loci with an alternative allele (genetics specification 2.1); every other
    /// locus of the genome map carries the reference allele in every copy.
    pub alleles: Vec<LocusAlleles>,
}

/// One line of the input log: an accepted input with its stamp (ADR-0004 section 4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRecord {
    pub t: SimTime,
    pub seq: InputSeq,
    pub input: Input,
}

/// Where an accepted input sits: `t` is the last closed second, `seq` its position in
/// the input log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputStamp {
    pub t: SimTime,
    pub seq: InputSeq,
}

/// Stable reason codes for a rejected input (no text, B16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectCode {
    /// `Start` on a world that has started.
    AlreadyStarted,
    /// `Start` after the clock moved; the world must start at its first second.
    ClockNotAtStart,
    /// A founder whose birth is not before the start (ADR-0002: negative birth times).
    FounderNotBornBeforeStart,
    /// More founders than the count in `core:started` can hold (2^32 - 1).
    TooManyFounders,
    /// An input other than `Start` on a world that has not started.
    NotStarted,
    /// An override block without a change.
    EmptyOverride,
    /// An override block that names a world parameter twice.
    OverrideParamTwice,
    /// Retired with the causes of death of `Start` (ADR-0017 section 2); never given, the
    /// index stays.
    DeathCauseTwice,
    /// Retired with the causes of death of `Start` (ADR-0017 section 2); never given, the
    /// index stays.
    DeathRateOutOfRange,
    /// A SimId that was never handed out.
    UnknownSim,
    /// A sim that is in the archive.
    SimNotAlive,
    /// A "woman" who is not female or a "man" who is not male.
    PartnersOfWrongSex,
    /// A sim that is in a partnership already.
    AlreadyPartnered,
    /// A couple of `Start` with a place beyond the list of founders.
    UnknownFounder,
    /// Retired with the fertility of `Start` (ADR-0017 section 4): never produced, its
    /// index stays.
    FertilityOutOfRange,
    /// A founder names a locus the genome map does not have.
    FounderUnknownLocus,
    /// A founder names a locus twice.
    FounderLocusTwice,
    /// A founder gives a locus more or fewer copies than its chromosome type and sex
    /// have, or an allele other than 0 and 1.
    FounderCopies,
    /// The years of the nutrition index of `Start` do not rise strictly.
    NutritionOrder,
    /// A nutrition index outside [0, 1], or of a year outside the range of time.
    NutritionOutOfRange,
    /// An override sets the nutrition index of a year before the fork year.
    NutritionYearPast,
    /// A partner under the market age of 16 in the second the partnership would form
    /// (ADR-0018 section 1; Dario: never below 16).
    PartnerTooYoung,
    /// A woman and a man with `φ ≥ 1/8` (ADR-0018 section 1).
    PartnersTooCloselyRelated,
    /// A `Start` with the capacity 0 (ADR-0020 section 6: `P / K` needs `K ≥ 1`).
    CapacityZero,
}

/// A rejected input changes nothing and takes no position in the input log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRejected {
    pub code: RejectCode,
}
