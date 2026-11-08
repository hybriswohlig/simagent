//! The social data (ADR-0030 section 6, ADR-0031): `data/social/social.json` with its types and
//! their checks. Everything is an integer; nothing here is a float. The kinds of memory with
//! their start strength, fade and valence, the start values of the bonds by role and their fades,
//! the topics of a talk with conditions and steps, the choice's company and avoid, the beliefs'
//! numbers and the limits. Only the memory part is used by the core so far; the rest is read and
//! checked so that the data hash does not move again when its slice comes.

use alloc::vec::Vec;
use core::fmt;
use core::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::data_id::DataId;

/// The six kinds of memory of phase 3, in the order of their numbers (ADR-0030 section 1,
/// "Was erinnert wird"): a talk, the birth of an own child, the death of a relative, the start
/// and the end of the own partnership, a belief changed by a refutation.
pub const MEMORY_KINDS: [&str; 6] = [
    "core:talk",
    "core:child_born",
    "core:relative_died",
    "core:partnership_began",
    "core:partnership_ended",
    "core:refuted",
];

/// The number of the kind `core:talk`.
pub const TALK: u16 = 0;
/// The number of the kind `core:child_born`.
pub const CHILD_BORN: u16 = 1;
/// The number of the kind `core:relative_died`.
pub const RELATIVE_DIED: u16 = 2;
/// The number of the kind `core:partnership_began`.
pub const PARTNERSHIP_BEGAN: u16 = 3;
/// The number of the kind `core:partnership_ended`.
pub const PARTNERSHIP_ENDED: u16 = 4;
/// The number of the kind `core:refuted`.
pub const REFUTED: u16 = 5;

const SCALE: RangeInclusive<u16> = 0..=1000;
const DELTA: RangeInclusive<i16> = -1000..=1000;
/// The oldest age a topic may name, the oldest age of the body contract (ADR-0014).
const OLDEST: u8 = 130;
const MOST_TOPICS: usize = 12;
const MOST_DEEDS: usize = 16;
/// The most company or avoid points of a choice: far below any critical need's 10^9 (ADR-0030
/// section 4).
const MOST_POINTS: RangeInclusive<u32> = 1..=100_000_000;
const MILLION: u32 = 1_000_000;

/// The checked social data, as `data/social/social.json` holds them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocialData {
    pub limits: Limits,
    pub memory: MemoryRules,
    pub bonds: BondRules,
    pub topics: Vec<Topic>,
    pub choice: ChoiceRules,
    pub beliefs: BeliefRules,
}

/// The limits (ADR-0030 sections 1 to 3 and 5, ADR-0031 sections 1 and 2): memories, bonds and
/// beliefs per person, the memories kept on moving out, the deeds a person remembers and for how
/// many house days, the age from which a person remembers and talks, the seconds from the
/// second arrival to a talk and between two talks of a pair, the talks per person and house day,
/// the entries of talks per person and house day and the room a talk needs in them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub memories: u16,
    pub bonds: u16,
    pub keep_on_move_out: u16,
    pub beliefs: u16,
    pub deeds: u16,
    pub deed_window_days: u16,
    pub min_age: u8,
    pub talk_delay_seconds: u32,
    pub pair_gap_seconds: u32,
    pub talks_per_day: u16,
    pub entries_per_day: u16,
    pub talk_entries: u16,
}

/// The memory rules (ADR-0030 section 1): what a recall adds, from which kinship a death is
/// remembered (`phi` at least this many eighths) and the kinds in the order of `MEMORY_KINDS`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRules {
    pub recall_step: u16,
    pub relative_phi_eighths: u8,
    pub kinds: Vec<MemoryKind>,
}

/// A kind of memory: the strength it starts with, its loss per day and its valence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryKind {
    pub id: DataId,
    pub strength: u16,
    pub fade: u16,
    pub valence: i16,
}

/// The bonds (ADR-0030 section 2): the loss of familiarity and of grudge per day and the six
/// values of a new bond by role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BondRules {
    pub familiarity_fade: u16,
    pub grudge_fade: u16,
    pub start: BondStarts,
}

/// The start values of a bond in the order affection, trust, respect, attraction, familiarity,
/// grudge, by the role of the other person to the one the bond belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BondStarts {
    pub partner: [u16; 6],
    pub parent: [u16; 6],
    pub child: [u16; 6],
    pub sibling: [u16; 6],
    pub kin: [u16; 6],
    pub housemate: [u16; 6],
}

/// A topic of a talk (ADR-0030 section 3): the ages of both, the furniture it may take place
/// at (none named means any sociable one), its condition and the steps of its outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topic {
    pub id: DataId,
    pub min_age: u8,
    pub max_age: u8,
    pub furniture: Vec<DataId>,
    pub condition: Condition,
    pub steps: Vec<Step>,
}

/// When a topic is allowed: always; the one who acts has an affection to the other of at least
/// the value; one of the two has a grudge of at least the value on the other; the two have a
/// common memory; one of them has material for gossip or a reason to lie (ADR-0031).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Always,
    AffectionFrom(u16),
    GrudgeFrom(u16),
    CommonMemory,
    Gossip,
}

/// One step of an outcome on a bond value: of both persons' bonds to each other, or of the bond
/// of the one the actor addresses (the praised, the teased, the one grudged) to the actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub who: Who,
    pub value: BondValue,
    pub delta: i16,
}

/// Whose bond a step changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Who {
    Both,
    Target,
}

/// The six values of a bond, in the order of the bond's array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BondValue {
    Affection,
    Trust,
    Respect,
    Attraction,
    Familiarity,
    Grudge,
}

/// What the choice adds (ADR-0030 section 4): the points for a seat next to a liked person and
/// the points off for one next to a distrusted person, and the thresholds: affection plus
/// familiarity at least `company_min_sum`, trust below `avoid_below_trust`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceRules {
    pub company: u32,
    pub avoid: u32,
    pub company_min_sum: u16,
    pub avoid_below_trust: u16,
}

/// The numbers of beliefs (ADR-0031): what a hearer takes over (`min(heard_cap,
/// floor(c · (heard_floor + trust) / heard_divisor))`), a confirmation, the certainty of a belief
/// concluded at a birth, a lie (the grudge it needs, the certainty it states and its rate in
/// parts per million), a refutation (its rate and its steps), the steps on hearing a deed aimed
/// at the hearer and any other, and the kinds of deed in the order of their numbers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeliefRules {
    pub heard_cap: u16,
    pub heard_floor: u16,
    pub heard_divisor: u16,
    pub confirm_step: u16,
    pub confirm_cap: u16,
    pub inferred_certainty: u16,
    pub lie_ppm: u32,
    pub lie_grudge: u16,
    pub lie_certainty: u16,
    pub refute_ppm: u32,
    pub refute_trust: i16,
    pub refute_grudge: i16,
    pub heard_aimed_trust: i16,
    pub heard_aimed_grudge: i16,
    pub heard_other_trust: i16,
    pub deeds: Vec<DataId>,
}

/// The rule a social file breaks (`social.*` codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocialError {
    /// A limit out of its range, or a keep-on-move-out above the memories.
    Limits,
    /// A recall step outside 1 to 1000 or a kinship outside 1 to 8 eighths.
    Memory,
    /// Not the six kinds of `MEMORY_KINDS` in their order, or one with a strength outside 1 to
    /// 1000, a fade or a valence out of range.
    MemoryKind,
    /// A fade above 1000 or a bond value above 1000.
    Bonds,
    /// Not one to twelve topics, a repeated id, a first topic that is not always allowed, ages
    /// out of order, a repeated furniture, or a threshold or step out of range.
    Topics,
    /// Company or avoid outside 1 to 10^8, a company threshold above 2000 (a sum of two values) or
    /// a trust threshold above 1000.
    Choice,
    /// A belief number out of range or a repeated or missing kind of deed.
    Beliefs,
}

impl fmt::Display for SocialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SocialError::Limits => "social.limits",
            SocialError::Memory => "social.memory",
            SocialError::MemoryKind => "social.memory_kind",
            SocialError::Bonds => "social.bonds",
            SocialError::Topics => "social.topics",
            SocialError::Choice => "social.choice",
            SocialError::Beliefs => "social.beliefs",
        })
    }
}

fn all_unique<T: Ord + Clone>(items: impl Iterator<Item = T>) -> bool {
    let mut seen: Vec<T> = items.collect();
    let count = seen.len();
    seen.sort_unstable();
    seen.dedup();
    seen.len() == count
}

impl SocialData {
    /// Each rule of the file, in the order of `SocialError`.
    pub fn checked(self) -> Result<SocialData, SocialError> {
        self.limits.checked()?;
        self.memory.checked()?;
        self.bonds.checked()?;
        self.check_topics()?;
        self.choice.checked()?;
        self.beliefs.checked()?;
        Ok(self)
    }

    fn check_topics(&self) -> Result<(), SocialError> {
        let topics = &self.topics;
        let step_fits = |step: &Step| DELTA.contains(&step.delta);
        let fits = |topic: &Topic| {
            let threshold = match topic.condition {
                Condition::AffectionFrom(at) | Condition::GrudgeFrom(at) => SCALE.contains(&at),
                Condition::Always | Condition::CommonMemory | Condition::Gossip => true,
            };
            topic.min_age <= topic.max_age
                && topic.max_age <= OLDEST
                && all_unique(topic.furniture.iter())
                && threshold
                && topic.steps.iter().all(step_fits)
        };
        let first = topics.first().map(|topic| topic.condition);
        let ok = (1..=MOST_TOPICS).contains(&topics.len())
            && first == Some(Condition::Always)
            && all_unique(topics.iter().map(|topic| &topic.id))
            && topics.iter().all(fits);
        ok.then_some(()).ok_or(SocialError::Topics)
    }

    /// The kind with this number, if the data has it.
    pub fn kind(&self, kind: u16) -> Option<&MemoryKind> {
        self.memory.kinds.get(usize::from(kind))
    }
}

impl Limits {
    fn checked(&self) -> Result<(), SocialError> {
        let ok = (1..=64).contains(&self.memories)
            && (1..=64).contains(&self.bonds)
            && (1..=self.memories).contains(&self.keep_on_move_out)
            && (1..=64).contains(&self.beliefs)
            && (1..=256).contains(&self.deeds)
            && (1..=364).contains(&self.deed_window_days)
            && (6..=OLDEST).contains(&self.min_age)
            && (60..=3600).contains(&self.talk_delay_seconds)
            && (self.talk_delay_seconds..=86_400).contains(&self.pair_gap_seconds)
            && (1..=64).contains(&self.talks_per_day)
            && (1..=1000).contains(&self.entries_per_day)
            && (1..=self.entries_per_day).contains(&self.talk_entries);
        ok.then_some(()).ok_or(SocialError::Limits)
    }
}

impl MemoryRules {
    fn checked(&self) -> Result<(), SocialError> {
        if !(1..=1000).contains(&self.recall_step) || !(1..=8).contains(&self.relative_phi_eighths)
        {
            return Err(SocialError::Memory);
        }
        let named = self.kinds.len() == MEMORY_KINDS.len()
            && (self.kinds.iter().zip(MEMORY_KINDS)).all(|(kind, id)| kind.id.as_str() == id);
        let fits = |kind: &MemoryKind| {
            (1..=1000).contains(&kind.strength)
                && kind.fade <= 1000
                && DELTA.contains(&kind.valence)
        };
        if named && self.kinds.iter().all(fits) {
            Ok(())
        } else {
            Err(SocialError::MemoryKind)
        }
    }
}

impl BondRules {
    fn checked(&self) -> Result<(), SocialError> {
        let start = &self.start;
        let roles = [
            &start.partner,
            &start.parent,
            &start.child,
            &start.sibling,
            &start.kin,
            &start.housemate,
        ];
        let ok = self.familiarity_fade <= 1000
            && self.grudge_fade <= 1000
            && roles
                .iter()
                .all(|role| role.iter().all(|v| SCALE.contains(v)));
        ok.then_some(()).ok_or(SocialError::Bonds)
    }
}

impl ChoiceRules {
    fn checked(&self) -> Result<(), SocialError> {
        let ok = MOST_POINTS.contains(&self.company)
            && MOST_POINTS.contains(&self.avoid)
            && self.company_min_sum <= 2000
            && SCALE.contains(&self.avoid_below_trust);
        ok.then_some(()).ok_or(SocialError::Choice)
    }
}

impl BeliefRules {
    fn checked(&self) -> Result<(), SocialError> {
        let caps = [
            self.heard_cap,
            self.confirm_cap,
            self.inferred_certainty,
            self.lie_certainty,
            self.lie_grudge,
        ];
        let steps = [
            self.refute_trust,
            self.refute_grudge,
            self.heard_aimed_trust,
            self.heard_aimed_grudge,
            self.heard_other_trust,
        ];
        let ok = caps.iter().all(|cap| SCALE.contains(cap))
            && self.heard_floor <= 1000
            && (1..=10_000).contains(&self.heard_divisor)
            && self.confirm_step <= 1000
            && self.lie_ppm <= MILLION
            && self.refute_ppm <= MILLION
            && steps.iter().all(|step| DELTA.contains(step))
            && (1..=MOST_DEEDS).contains(&self.deeds.len())
            && all_unique(self.deeds.iter());
        ok.then_some(()).ok_or(SocialError::Beliefs)
    }
}
