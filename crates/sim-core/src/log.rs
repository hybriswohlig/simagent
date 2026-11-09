//! The domain log (ADR-0004 sections 6 and 7): pure output that the core never reads
//! back. Every entry names its causes; the payload variant name is the stable kind code.
//! New variants of `LogPayload` and `Cause` only go to the end (postcard encodes the
//! variant index; `tests/golden/log_codes.txt` pins the order).

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::data_id::{Code, DataId};
use crate::genome::SexChromosome;
use crate::genome::meiosis::{Mutation, SegmentMeiosis};
use crate::hash::StableHasher;
use crate::house::data::Need;
use crate::house::state::Duty;
use crate::ids::{EventId, HouseholdId, InputSeq, PartnershipId, SimId};
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
    /// One meiosis of a conception (ADR-0004 section 7, genetics specification 2.3 and
    /// 2.4); subject the parent. Per segment the start strand and the crossover
    /// positions, the sex chromosome passed on (always X from the mother) and the new
    /// mutations in their order.
    #[serde(rename = "core:meiosis")]
    Meiosis {
        genome_key: GenomeKey,
        segments: Vec<SegmentMeiosis>,
        sex_chromosome: SexChromosome,
        mutations: Vec<Mutation>,
    },
    /// The child with this lineage key was born (creation entry of a SimId; subjects the
    /// child, the mother and the father).
    #[serde(rename = "core:birth")]
    Birth { lineage_key: LineageKey },
    /// A woman and a man (the subjects, in this order) separated: their partnership ended
    /// (ADR-0018 section 3).
    #[serde(rename = "core:separated")]
    Separated { partnership: PartnershipId },
    /// A household formed (creation entry of a HouseholdId; subjects: the members it
    /// starts with, ADR-0018 section 4).
    #[serde(rename = "core:household_formed")]
    HouseholdFormed { household: HouseholdId },
    /// A person (the subject) moved into this household.
    #[serde(rename = "core:moved")]
    Moved { household: HouseholdId },
    /// A person (the subject) left the town with this household above the soft capacity
    /// (ADR-0020 section 6).
    #[serde(rename = "core:emigrated")]
    Emigrated { household: HouseholdId },
    /// A person of the pool came into the town (creation entry of a SimId; ADR-0020
    /// section 5): a founder at `Start` or an immigrant.
    #[serde(rename = "core:arrived")]
    Arrived { lineage_key: LineageKey },
    /// A person (the subject) became affected by this hereditary disease in the run
    /// (ADR-0017 section 3): `since_year` is the calendar year of its onset second and
    /// `age_years` the onset in whole years, as the genetics card shows them.
    #[serde(rename = "core:disease_onset")]
    DiseaseOnset {
        disease: Code,
        since_year: i64,
        age_years: i64,
    },
    /// A shock from its template (ADR-0023 section 3 and its addendum): the calendar years
    /// it sets, from `first_year` to `last_year`; one `core:param_changed` per year follows,
    /// caused by this entry.
    #[serde(rename = "core:shock_triggered")]
    ShockTriggered {
        shock: DataId,
        first_year: i64,
        last_year: i64,
    },
    /// A woman and a man (the subjects, in this order) spend time together: the input
    /// opened the window of their partnership (ADR-0023, addendum of 2026-11-01, W4).
    #[serde(rename = "core:time_together")]
    TimeTogether { partnership: PartnershipId },
    /// A household (all its members as subjects, by SimId) moves into `house`, or one
    /// person (the subject) comes into a household in its house (ADR-0024 sections 1 and 2).
    #[serde(rename = "core:moved_in")]
    MovedIn {
        household: HouseholdId,
        house: DataId,
    },
    /// A household (all its members) moves out of its house, or one member (the subject)
    /// leaves it.
    #[serde(rename = "core:moved_out")]
    MovedOut { household: HouseholdId },
    /// A person's first day in the house begins with these values of hunger, energy,
    /// bladder, hygiene and fun (ADR-0024 section 1, ADR-0026 section 1).
    #[serde(rename = "core:day_started")]
    DayStarted { needs: [u16; 5] },
    /// A person (the subject) chose `offer` of `object` in `house` and walks to it from
    /// `room`, `walk_secs` long, holding its place `slot` (ADR-0026 sections 4 and 6): the
    /// needs of the choice and the objects without a free place, so that "Why?" can compute
    /// the choice again; `commanded` if a command chose it. In the person's night the objects
    /// with a place a small child holds, `held`, and the beds of its tier, `tier` (none for a
    /// command; ADR-0026, addendum of 2026-11-04 after the playtest); by day both are empty.
    /// `filler` if nothing brought enough and the person took a time-filler instead of idling
    /// (ADR-0026, addendum of 2026-11-04 on comfort and the time-filler).
    #[serde(rename = "core:action_started")]
    ActionStarted {
        house: DataId,
        object: u16,
        offer: u16,
        slot: u8,
        room: u8,
        walk_secs: u32,
        needs: [u16; 5],
        busy: Vec<u16>,
        commanded: bool,
        held: Vec<u16>,
        tier: Vec<u16>,
        filler: bool,
    },
    /// A person reached the object of its action and begins it.
    #[serde(rename = "core:reached")]
    Reached { object: u16 },
    /// A person's action ended for `reason`, with the needs of that second.
    #[serde(rename = "core:action_ended")]
    ActionEnded {
        object: u16,
        offer: u16,
        reason: EndReason,
        needs: [u16; 5],
    },
    /// A person chose nothing: no free offer brought enough points (ADR-0026 section 5). The
    /// house, the room, the needs and the objects without a free place as for a start;
    /// `waits` the object it waits for; `held` and `tier` as for a start; `too_short` by day the
    /// end its time-filler would have had, under `fallback_min_seconds` away, so it took none
    /// (ADR-0026, addendum of 2026-11-05 on the time-filler's length).
    #[serde(rename = "core:idle")]
    Idle {
        house: DataId,
        room: u8,
        needs: [u16; 5],
        busy: Vec<u16>,
        waits: Option<u16>,
        held: Vec<u16>,
        tier: Vec<u16>,
        too_short: Option<SimTime>,
    },
    /// A person's need fell to `level`, 150 or 50 (ADR-0026 section 4).
    #[serde(rename = "core:need_critical")]
    NeedCritical { need: Need, level: u16 },
    /// A person's duty of the day: it leaves for `place` at `depart` and is back at `back`
    /// (ADR-0024 section 4); the reason of its wake, departure and return.
    #[serde(rename = "core:day_planned")]
    DayPlanned {
        place: Duty,
        depart: SimTime,
        back: SimTime,
    },
    /// A person left the house for `place`, `late_secs` after its plan.
    #[serde(rename = "core:left_home")]
    LeftHome { place: Duty, late_secs: u32 },
    /// A person came back from `place`, at the entrance.
    #[serde(rename = "core:came_home")]
    CameHome { place: Duty },
    /// A person never left for `place`: its departure still waited at its return.
    #[serde(rename = "core:missed")]
    Missed { place: Duty },
    /// A person's need fell to 0, or a jump on coming home or a change of fun put it there
    /// (ADR-0026, addendum of 2026-11-04 on a need at 0).
    #[serde(rename = "core:need_empty")]
    NeedEmpty { need: Need },
    /// A person whose energy reached 0 lies on the floor of `room` until it gets up.
    #[serde(rename = "core:collapsed")]
    Collapsed { room: u8 },
    /// A person's bladder reached 0: its needs after, the bladder full and hygiene at most
    /// the data's value.
    #[serde(rename = "core:accident")]
    Accident { needs: [u16; 5] },
    /// A person's reserve at hunger 0 fell to half its `capacity`: it is weak (ADR-0026,
    /// addendum of 2026-11-06 on hunger at 0).
    #[serde(rename = "core:weakened")]
    Weakened { capacity: u16 },
    /// A person's reserve of `capacity` fell to 0: it is wasted.
    #[serde(rename = "core:wasted")]
    Wasted { capacity: u16 },
    /// A person (the subject) took a memory of `kind` (the number of its kind in the social
    /// data) into its list: what happened at `at` with the persons `with` (ascending, a single
    /// one first), `strength` as it starts (ADR-0030 section 1). The domain log is never read
    /// back: the memory itself lives in the state.
    #[serde(rename = "core:remembered")]
    Remembered {
        kind: u16,
        with: [Option<SimId>; 2],
        at: SimTime,
        strength: u16,
    },
    /// A person (the subject) lost a memory: it faded to 0, or a newer one displaced it from a
    /// full list (ADR-0030 section 1).
    #[serde(rename = "core:forgot")]
    Forgot {
        kind: u16,
        with: [Option<SimId>; 2],
        at: SimTime,
        reason: ForgetReason,
    },
    /// A person (the subject) formed a bond to `other` with the start `values` (affection,
    /// trust, respect, attraction, familiarity, grudge), each from 0 to 1000 (ADR-0030
    /// section 2).
    #[serde(rename = "core:bond_formed")]
    BondFormed { other: SimId, values: [u16; 6] },
    /// A person (the subject) changed its bond to `other`: the six values at this second
    /// before the step and after it (ADR-0030 section 2). A later slice writes it; the
    /// variant stands here so the log order is the one of that section.
    #[serde(rename = "core:bond_changed")]
    BondChanged {
        other: SimId,
        before: [u16; 6],
        after: [u16; 6],
    },
    /// A person (the subject) lost its bond to `other`: a new bond took its place in a full
    /// list (ADR-0030 section 2).
    #[serde(rename = "core:bond_dropped")]
    BondDropped { other: SimId },
}

/// Why a memory was lost: its strength reached 0, or a newer one took its place in a full list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgetReason {
    Faded,
    Displaced,
}

/// Why an action ended (ADR-0026 section 4): its length was over, the need it fills was
/// full, the person left the house, a need it does not raise turned critical, the day's
/// duty came (the departure, or the wake during sleep), a command (ADR-0024 section 5), or
/// the person's energy reached 0 (ADR-0026, addendum of 2026-11-04 on a need at 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Done,
    Full,
    Left,
    Critical,
    Obligation,
    Command,
    Collapse,
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
