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
    /// Sets one world parameter at a stop of the run (ADR-0023 section 2); in phase 1
    /// only the switch of the hereditary diseases.
    SetWorldParam(SetWorldParam),
    /// Triggers a shock from its template in the data (ADR-0023 section 3).
    TriggerShock(TriggerShock),
}

/// The start of a run (ADR-0004 section 4 and its addendum of 2026-10-30). The data's hash
/// stands in the log's header, a shock is an input of its own, and a run's stop is the end
/// line of its input log, no field here; a mod list arrives as a field with its first reader.
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
    /// A town founded from the pool of the data (ADR-0020 section 5); `None` for the
    /// founders above. Required in JSON like `capacity` (`null` for none; scenario format 5).
    #[serde(deserialize_with = "Option::deserialize")]
    pub pool: Option<PoolStart>,
}

/// The founding of a town from the pool of the data (ADR-0020 section 5; design note of
/// the arrival, section 3): the drawn households travel in the input, so that the input
/// log carries them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolStart {
    /// `Pool::hash` of the data's pool.
    #[serde(with = "crate::hex_u64")]
    pub pool_hash: u64,
    pub n_start: u64,
    /// Whether households come every year (`(n_start + 50) div 100` persons).
    pub immigration: bool,
    /// A village of P4 (ADR-0020 section 8, G4): `n_start / 2` persons of each sex, each alone,
    /// instead of whole households; immigration off.
    pub village: bool,
    /// The households of the draw for occasion 0, in its order, exactly as in the pool.
    pub households: Vec<crate::pool::PoolHousehold>,
}

impl PoolStart {
    /// The pool start of a town of `n_start` founded from the data's pool (ADR-0020 section
    /// 5): the pool's hash and the households of the draw for occasion 0 until their members
    /// reach `n_start`, exactly what `Start` checks; `None` for data without a pool. The CLI
    /// resolves a scenario's pool with it.
    pub fn drawn(
        data: &crate::data::Data,
        world_seed: u64,
        n_start: u64,
        immigration: bool,
    ) -> Option<PoolStart> {
        let held = data.pool()?;
        let (taken, _) = crate::pool::take_households(
            held.pool(),
            world_seed,
            0,
            &alloc::collections::BTreeSet::new(),
            n_start,
        );
        Some(PoolStart {
            pool_hash: held.hash(),
            n_start,
            immigration,
            village: false,
            households: taken
                .iter()
                .map(|&index| held.pool().households[index].clone())
                .collect(),
        })
    }
}

impl PoolStart {
    /// The pool start of a village of `n_start` (ADR-0020 section 8, G4): the data's pool hash
    /// and the persons of `pool::village_households`, `n_start / 2` of each sex, immigration
    /// off; `None` for data without a pool, an odd `n_start` or too few candidates.
    pub fn village(data: &crate::data::Data, world_seed: u64, n_start: u64) -> Option<PoolStart> {
        if !n_start.is_multiple_of(2) {
            return None;
        }
        let held = data.pool()?;
        Some(PoolStart {
            pool_hash: held.hash(),
            n_start,
            immigration: false,
            village: true,
            households: crate::pool::village_households(held.pool(), world_seed, n_start / 2)?,
        })
    }
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

/// One world parameter with its new value, set at a stop of the run (ADR-0023 section 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetWorldParam {
    pub change: ParamChange,
}

/// A shock by the id of its template (ADR-0023 section 3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerShock {
    pub shock: DataId,
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
    /// A `Start` with a pool that also has founders or couples of its own.
    StartPoolAndFounders,
    /// A `Start` with a pool, and the data hold none or another one.
    StartPoolMissing,
    /// A `Start` with a pool for 0 persons.
    StartPoolNStart,
    /// A `Start` whose households are not the draw of the pool.
    StartPoolHouseholds,
    /// A village with an odd `n_start` (G4: as many women as men).
    StartPoolVillageNStart,
    /// A village with immigration (ADR-0020 section 8: none).
    StartPoolVillageImmigration,
    /// A village whose pool holds too few women or men of 18 to before 41.
    StartPoolVillageCandidates,
    /// An input stamped at the last second of time: there is no next second for it to
    /// act in (ADR-0023 section 1).
    EndOfTime,
    /// A world rule for a parameter that is not to be set at run time (ADR-0023
    /// section 2: in phase 1 only the switch of the hereditary diseases).
    ParamNotSettable,
    /// A world rule that sets the value the parameter has (ADR-0023 section 2).
    ParamUnchanged,
    /// A shock whose template the data do not hold (ADR-0023 section 3).
    ShockUnknown,
    /// A shock over a year that already has a hunger, an index below 1 (ADR-0023
    /// section 3).
    ShockOverlaps,
    /// A woman and a man of whom one is an ancestor of the other, at any depth (ADR-0018
    /// section 8).
    PartnersInDirectLine,
}

/// A rejected input changes nothing and takes no position in the input log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRejected {
    pub code: RejectCode,
}
