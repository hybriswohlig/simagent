//! The pool of the pre-run (ADR-0020 sections 4 and 5; design note of the pool file): the
//! households of the pool run at its last second `E_p` that have a member of 18 or more,
//! with what each person brings into the town, the alleles of the seven loci the pre-run
//! left at the reference allele, and the ancestors up to the great-grandparents.
//!
//! `extract` builds it from the world at `E_p` and the genome keys of the run's log;
//! `Pool::to_bytes` writes the file (`b"SIMPOOL"`, the format version, postcard) and
//! `Pool::from_bytes` reads it back with its checks. The arrival in the town is the next
//! slice.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::data::{Chromosome, Data};
use crate::data_id::{DataId, str_key};
use crate::finite::FiniteF64;
use crate::genome::{Genome, Sex};
use crate::hash::stable_hash64;
use crate::ids::SimId;
use crate::lineage::{GenomeKey, LineageKey};
use crate::log::{LogEntry, LogPayload, Subject};
use crate::phenotype;
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::state::{self, DomainState, Slot};
use crate::time::{MONTH, SimTime, YEAR};
use crate::world::World;

/// The format of the pool file.
pub const POOL_FORMAT: u64 = 2;

/// The version of the load-time extension that writes the temperament loci into a pool
/// whose map declares them. Applying one version twice writes the same alleles. The
/// constant is not stored in the pool file.
pub const TEMPERAMENT_POOL_EXTENSION: u32 = 1;

/// The seven loci the pre-run leaves at the reference allele and the pool assigns
/// (ADR-0020 section 4).
pub fn pool_loci() -> [DataId; 7] {
    [
        "core:disease_ar",
        "core:disease_ad_late",
        "core:disease_ad_partial",
        "core:disease_xr",
        "core:disease_mt",
        "core:mc1r",
        "core:herc2",
    ]
    .map(|id| DataId::new(id).unwrap_or_else(|_| panic!("invariant pool.locus_id")))
}

/// The first bytes of a pool file.
pub const MAGIC: &[u8; 7] = b"SIMPOOL";

/// The pool of a pool run (design note of the pool file, section 7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pool {
    pub pool_seed: u64,
    /// `E_p`, the last second of the pool run.
    pub frozen_at: SimTime,
    /// The hash of the pool run's data.
    pub data_hash: u64,
    /// The hash of the frozen data the pool was extracted from (positions of the seven loci,
    /// the Mendelian diseases): a town loads the pool only with these data (Grok K7 and P1).
    pub frozen_hash: u64,
    /// In the order of their `HouseholdId` in the pool run.
    pub households: Vec<PoolHousehold>,
    /// Ascending by lineage key.
    pub ancestors: Vec<Ancestor>,
}

/// A household of the pool: its members ascending by birth, then lineage key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolHousehold {
    pub members: Vec<PoolPerson>,
    pub partnership: Option<PoolPartnership>,
    /// By the place of the mother among the members.
    pub pregnancies: Vec<PoolPregnancy>,
}

/// A living person of the pool (design note, section 5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolPerson {
    pub lineage_key: LineageKey,
    pub genome_key: GenomeKey,
    pub sex: Sex,
    pub genome: Genome,
    /// The birth second in the pool run.
    pub born: SimTime,
    pub mother: Option<LineageKey>,
    pub father: Option<LineageKey>,
    pub conceptions: u32,
    pub births: u32,
    pub partnerships: u32,
    pub moved_out: bool,
    pub carer: Option<LineageKey>,
    pub social_parents: Vec<LineageKey>,
    pub afflictions: Vec<PoolAffliction>,
    /// The next conception attempt of her monthly series.
    pub conception_at: Option<SimTime>,
    /// The due separation of her open partnership (the woman's).
    pub separation_at: Option<SimTime>,
    pub leave_home_at: Option<SimTime>,
    /// The birth that ends her pregnancy.
    pub birth_at: Option<SimTime>,
}

/// A hereditary disease a person has: the `str_key` of its code and the onset in years.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolAffliction {
    pub disease: u64,
    pub onset: FiniteF64,
}

/// The open partnership of a household; both partners are members.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolPartnership {
    pub woman: LineageKey,
    pub man: LineageKey,
    pub formed: SimTime,
}

/// A running pregnancy of a member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolPregnancy {
    pub mother: LineageKey,
    pub father: LineageKey,
    /// The mother's conception number of it.
    pub conception: u32,
    pub conceived: SimTime,
    /// In their multiple order.
    pub children: Vec<PoolUnborn>,
}

/// A child of a running pregnancy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolUnborn {
    pub lineage_key: LineageKey,
    pub genome_key: GenomeKey,
    pub sex: Sex,
    pub genome: Genome,
    pub afflictions: Vec<PoolAffliction>,
}

/// A record of the ancestors (G3): a parent is named only if it is itself a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ancestor {
    pub lineage_key: LineageKey,
    pub born: SimTime,
    pub mother: Option<LineageKey>,
    pub father: Option<LineageKey>,
}

/// Why a pool file is rejected (design note, section 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolError {
    Magic,
    Version,
    Decode,
    TrailingBytes,
    HouseholdWithoutAdult,
    MembersOutOfOrder,
    AncestorsOutOfOrder,
    ParentNotARecord,
    PartnershipNotMembers,
    /// `frozen_at` is not a year start.
    FrozenAt,
    /// The woman of a partnership is not female, or its man not male.
    PartnershipSex,
    /// A pregnancy's mother is not a female member, two have one mother, they do not
    /// follow the members' order, or one has no child.
    PregnancyMother,
    /// A member plans a birth without a pregnancy, or a pregnancy has no birth planned.
    BirthAt,
    /// A conception series of another than the partnership's woman, or of a pregnant one.
    ConceptionAt,
    /// A conception series off the month grid.
    ConceptionMonth,
    /// A separation planned by another than the partnership's woman.
    SeparationAt,
    /// Leaving home planned for one moved out, or missing for one at home.
    LeaveHomeAt,
    /// A carried time at or before `frozen_at` (ADR-0020 section 3).
    CarriedTime,
    /// A birth, a partnership or a conception after `frozen_at`.
    PastTime,
    /// The file decodes and passes every check but is not written this way (an overlong
    /// varint, review): its hash would not be the hash of what the core writes.
    NonCanonical,
}

/// The code of the rule a file breaks, as the data check reports it.
impl core::fmt::Display for PoolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            PoolError::Magic => "pool.magic",
            PoolError::Version => "pool.version",
            PoolError::Decode => "pool.decode",
            PoolError::TrailingBytes => "pool.trailing_bytes",
            PoolError::HouseholdWithoutAdult => "pool.household_without_adult",
            PoolError::MembersOutOfOrder => "pool.members_out_of_order",
            PoolError::AncestorsOutOfOrder => "pool.ancestors_out_of_order",
            PoolError::ParentNotARecord => "pool.parent_not_a_record",
            PoolError::PartnershipNotMembers => "pool.partnership_not_members",
            PoolError::FrozenAt => "pool.frozen_at",
            PoolError::PartnershipSex => "pool.partnership_sex",
            PoolError::PregnancyMother => "pool.pregnancy_mother",
            PoolError::BirthAt => "pool.birth_at",
            PoolError::ConceptionAt => "pool.conception_at",
            PoolError::ConceptionMonth => "pool.conception_month",
            PoolError::SeparationAt => "pool.separation_at",
            PoolError::LeaveHomeAt => "pool.leave_home_at",
            PoolError::CarriedTime => "pool.carried_time",
            PoolError::PastTime => "pool.past_time",
            PoolError::NonCanonical => "pool.non_canonical",
        })
    }
}

/// Of age at `at`: 18 years or more since `born`.
fn adult(born: SimTime, at: SimTime) -> bool {
    at.as_secs()
        .checked_sub(born.as_secs())
        .is_some_and(|age| age >= 18 * YEAR.as_secs())
}

impl Pool {
    /// The file: the magic, the format version as a varint, the pool as postcard.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        let version =
            postcard::to_allocvec(&POOL_FORMAT).unwrap_or_else(|_| panic!("invariant pool.encode"));
        bytes.extend(version);
        bytes.extend(
            postcard::to_allocvec(self).unwrap_or_else(|_| panic!("invariant pool.encode")),
        );
        bytes
    }

    /// The hash of the file: xxh3-64 of its bytes.
    pub fn hash(&self) -> u64 {
        stable_hash64(&self.to_bytes())
    }

    /// Reads a pool file and checks it (design note, section 7).
    pub fn from_bytes(bytes: &[u8]) -> Result<Pool, PoolError> {
        let body = bytes
            .strip_prefix(MAGIC.as_slice())
            .ok_or(PoolError::Magic)?;
        let (version, body) =
            postcard::take_from_bytes::<u64>(body).map_err(|_| PoolError::Version)?;
        if version != POOL_FORMAT {
            return Err(PoolError::Version);
        }
        let (pool, rest) =
            postcard::take_from_bytes::<Pool>(body).map_err(|_| PoolError::Decode)?;
        if !rest.is_empty() {
            return Err(PoolError::TrailingBytes);
        }
        pool.check()?;
        if pool.to_bytes() != bytes {
            return Err(PoolError::NonCanonical);
        }
        Ok(pool)
    }

    fn check(&self) -> Result<(), PoolError> {
        for household in &self.households {
            if !household
                .members
                .iter()
                .any(|member| adult(member.born, self.frozen_at))
            {
                return Err(PoolError::HouseholdWithoutAdult);
            }
            let ascending = household.members.windows(2).all(|pair| {
                (pair[0].born, pair[0].lineage_key) < (pair[1].born, pair[1].lineage_key)
            });
            if !ascending {
                return Err(PoolError::MembersOutOfOrder);
            }
        }
        let ascending = self
            .ancestors
            .windows(2)
            .all(|pair| pair[0].lineage_key < pair[1].lineage_key);
        if !ascending {
            return Err(PoolError::AncestorsOutOfOrder);
        }
        let records: BTreeSet<LineageKey> = self
            .ancestors
            .iter()
            .map(|record| record.lineage_key)
            .collect();
        let known = |parent: Option<LineageKey>| parent.is_none_or(|key| records.contains(&key));
        let members = self
            .households
            .iter()
            .flat_map(|household| household.members.iter())
            .all(|member| known(member.mother) && known(member.father));
        let ancestors = self
            .ancestors
            .iter()
            .all(|record| known(record.mother) && known(record.father));
        let pregnancies = self
            .households
            .iter()
            .flat_map(|household| household.pregnancies.iter())
            .all(|pregnancy| known(Some(pregnancy.mother)) && known(Some(pregnancy.father)));
        if !(members && ancestors && pregnancies) {
            return Err(PoolError::ParentNotARecord);
        }
        for household in &self.households {
            if let Some(pair) = household.partnership {
                let member =
                    |key: LineageKey| household.members.iter().any(|m| m.lineage_key == key);
                if !(member(pair.woman) && member(pair.man)) {
                    return Err(PoolError::PartnershipNotMembers);
                }
            }
        }
        self.check_records()
    }

    /// The facts of the records the town relies on (design note of the pool's records,
    /// section 1), in the order of their codes.
    fn check_records(&self) -> Result<(), PoolError> {
        if self.frozen_at.as_secs().rem_euclid(YEAR.as_secs()) != 0 {
            return Err(PoolError::FrozenAt);
        }
        let sex_of = |household: &PoolHousehold, key: LineageKey| {
            household
                .members
                .iter()
                .find(|m| m.lineage_key == key)
                .map(|m| m.sex)
        };
        for household in &self.households {
            if let Some(pair) = household.partnership
                && (sex_of(household, pair.woman) != Some(Sex::Female)
                    || sex_of(household, pair.man) != Some(Sex::Male))
            {
                return Err(PoolError::PartnershipSex);
            }
        }
        for household in &self.households {
            let place =
                |key: LineageKey| household.members.iter().position(|m| m.lineage_key == key);
            let places: Option<Vec<usize>> = household
                .pregnancies
                .iter()
                .map(|p| {
                    place(p.mother).filter(|_| {
                        sex_of(household, p.mother) == Some(Sex::Female) && !p.children.is_empty()
                    })
                })
                .collect();
            if !places.is_some_and(|places| places.windows(2).all(|pair| pair[0] < pair[1])) {
                return Err(PoolError::PregnancyMother);
            }
        }
        let pregnant = |household: &PoolHousehold, member: &PoolPerson| {
            household
                .pregnancies
                .iter()
                .any(|p| p.mother == member.lineage_key)
        };
        let woman = |household: &PoolHousehold, member: &PoolPerson| {
            household
                .partnership
                .is_some_and(|pair| pair.woman == member.lineage_key)
        };
        let members = || {
            self.households
                .iter()
                .flat_map(|h| h.members.iter().map(move |m| (h, m)))
        };
        if members().any(|(h, m)| m.birth_at.is_some() != pregnant(h, m)) {
            return Err(PoolError::BirthAt);
        }
        if members().any(|(h, m)| m.conception_at.is_some() && (!woman(h, m) || pregnant(h, m))) {
            return Err(PoolError::ConceptionAt);
        }
        if members().any(|(_, m)| {
            m.conception_at
                .is_some_and(|t| t.as_secs().rem_euclid(MONTH.as_secs()) != 0)
        }) {
            return Err(PoolError::ConceptionMonth);
        }
        if members().any(|(h, m)| m.separation_at.is_some() && !woman(h, m)) {
            return Err(PoolError::SeparationAt);
        }
        if members().any(|(_, m)| m.leave_home_at.is_some() == m.moved_out) {
            return Err(PoolError::LeaveHomeAt);
        }
        let carried = |m: &PoolPerson| {
            [
                m.conception_at,
                m.birth_at,
                m.separation_at,
                m.leave_home_at,
            ]
            .into_iter()
            .flatten()
            .all(|t| t > self.frozen_at)
        };
        if members().any(|(_, m)| !carried(m)) {
            return Err(PoolError::CarriedTime);
        }
        let past = |at: SimTime| at <= self.frozen_at;
        let all_past = self.households.iter().all(|h| {
            h.members.iter().all(|m| past(m.born))
                && h.partnership.is_none_or(|pair| past(pair.formed))
                && h.pregnancies.iter().all(|p| past(p.conceived))
        });
        if !all_past {
            return Err(PoolError::PastTime);
        }
        Ok(())
    }
}

/// The candidates of the pool's draws (ADR-0020 section 5; design note of the arrival,
/// section 3): per household with a member of 18 or more at `frozen_at`, the smallest
/// lineage key of such a member, with the household's index. They depend on the pool
/// alone, so the load of a save, which replays every draw, finds them once.
pub struct Candidates<'a> {
    pool: &'a Pool,
    households: BTreeMap<LineageKey, usize>,
}

impl<'a> Candidates<'a> {
    /// The candidates of `pool`.
    pub fn of(pool: &'a Pool) -> Self {
        let households = pool
            .households
            .iter()
            .enumerate()
            .filter_map(|(index, household)| {
                household
                    .members
                    .iter()
                    .filter(|member| adult(member.born, pool.frozen_at))
                    .map(|member| member.lineage_key)
                    .min()
                    .map(|candidate| (candidate, index))
            })
            .collect();
        Self { pool, households }
    }

    /// The candidates in the order of the draw on `occasion` (ADR-0001 section 5):
    /// ascending by `(rank, candidate)` with `POOL_RANK` and the subject `WORLD`.
    fn order(&self, world_seed: u64, occasion: u64) -> Vec<LineageKey> {
        Draws::new(WorldSeed::new(world_seed)).ranked(
            tags::POOL_RANK,
            SubjectKey::WORLD,
            occasion,
            self.households.keys().copied(),
        )
    }

    /// The households in the order of the draw on `occasion`. Indices into `households`.
    pub fn ranked(&self, world_seed: u64, occasion: u64) -> Vec<usize> {
        self.order(world_seed, occasion)
            .into_iter()
            .map(|candidate| self.households[&candidate])
            .collect()
    }

    /// The households not in `used`, in the order of the draw on `occasion`, until their
    /// members reach or exceed `persons`; the second value says whether they do (with too
    /// few households left, all of them and `false`). Only the households it goes through
    /// are looked up.
    pub fn take(
        &self,
        world_seed: u64,
        occasion: u64,
        used: &BTreeSet<usize>,
        persons: u64,
    ) -> (Vec<usize>, bool) {
        let mut taken = Vec::new();
        let mut count = 0_u64;
        for candidate in self.order(world_seed, occasion) {
            if count >= persons {
                break;
            }
            let index = self.households[&candidate];
            if used.contains(&index) {
                continue;
            }
            count = count.saturating_add(
                u64::try_from(self.pool.households[index].members.len()).unwrap_or(u64::MAX),
            );
            taken.push(index);
        }
        (taken, count >= persons)
    }
}

/// The households of the pool in the order of the draw on `occasion`: `Candidates::ranked`.
pub fn ranked_households(pool: &Pool, world_seed: u64, occasion: u64) -> Vec<usize> {
    Candidates::of(pool).ranked(world_seed, occasion)
}

/// The persons of a village (ADR-0020 section 8, G4; Grok V1 and V2): per sex the `per_sex`
/// members aged from their 18th birthday to before their 41st at `frozen_at`, in the order of
/// the draw on occasion 1 (`POOL_RANK`, subject `WORLD`, candidate the lineage key), women
/// first; each alone in a household of their own, without partnership, pregnancy, carer or
/// carried plans, moved out. `None` when a sex has fewer candidates.
pub fn village_households(
    pool: &Pool,
    world_seed: u64,
    per_sex: u64,
) -> Option<Vec<PoolHousehold>> {
    let draws = Draws::new(WorldSeed::new(world_seed));
    let wanted = usize::try_from(per_sex).ok()?;
    let in_age = |born: SimTime| {
        pool.frozen_at
            .as_secs()
            .checked_sub(born.as_secs())
            .is_some_and(|age| age >= 18 * YEAR.as_secs() && age < 41 * YEAR.as_secs())
    };
    let mut households = Vec::new();
    for sex in [Sex::Female, Sex::Male] {
        let candidates: BTreeMap<LineageKey, &PoolPerson> = pool
            .households
            .iter()
            .flat_map(|household| household.members.iter())
            .filter(|member| member.sex == sex && in_age(member.born))
            .map(|member| (member.lineage_key, member))
            .collect();
        let ranked = draws.ranked(
            tags::POOL_RANK,
            SubjectKey::WORLD,
            1,
            candidates.keys().copied(),
        );
        if ranked.len() < wanted {
            return None;
        }
        for key in ranked.into_iter().take(wanted) {
            let mut person = candidates[&key].clone();
            person.carer = None;
            person.conception_at = None;
            person.birth_at = None;
            person.separation_at = None;
            person.leave_home_at = None;
            person.moved_out = true;
            households.push(PoolHousehold {
                members: vec![person],
                partnership: None,
                pregnancies: Vec::new(),
            });
        }
    }
    Some(households)
}

/// One draw of the pool on its own: `Candidates::take`.
pub fn take_households(
    pool: &Pool,
    world_seed: u64,
    occasion: u64,
    used: &BTreeSet<usize>,
    persons: u64,
) -> (Vec<usize>, bool) {
    Candidates::of(pool).take(world_seed, occasion, used, persons)
}

/// The genome keys of the children conceived in a run, read from its log (design note,
/// section 1): a child whose lineage key is the genome key of a `core:meiosis` entry is the
/// first child of its zygote; any other is an identical multiple of the child of the
/// conception entry before it in the same conception.
#[derive(Clone, Debug, Default)]
pub struct GenomeKeys {
    keys: BTreeMap<LineageKey, GenomeKey>,
    /// The last child seen and its conception (second and mother).
    last: Option<(SimTime, SimId, LineageKey)>,
}

impl GenomeKeys {
    pub fn new() -> GenomeKeys {
        GenomeKeys::default()
    }

    /// Reads the entries of one batch of the log, in order.
    pub fn observe(&mut self, entries: &[LogEntry]) {
        for entry in entries {
            match &entry.payload {
                LogPayload::Conception { lineage_key } => {
                    let Some(Subject::Sim(mother)) = entry.subjects.first().copied() else {
                        panic!("invariant pool.conception_subjects");
                    };
                    // A later child of the same conception is taken for an identical
                    // multiple of the one before until its own meiosis says otherwise.
                    let key = match self.last {
                        Some((t, woman, previous)) if t == entry.t && woman == mother => self
                            .keys
                            .get(&previous)
                            .copied()
                            .unwrap_or_else(|| GenomeKey::of_zygote(previous)),
                        _ => GenomeKey::of_zygote(*lineage_key),
                    };
                    self.keys.insert(*lineage_key, key);
                    self.last = Some((entry.t, mother, *lineage_key));
                }
                LogPayload::Meiosis { genome_key, .. } => {
                    if let Some((_, _, child)) = self.last
                        && child.get() == genome_key.get()
                    {
                        self.keys.insert(child, GenomeKey::of_zygote(child));
                    }
                }
                _ => {}
            }
        }
    }

    /// The genome key of `child`; a founder (no conception in the log) is its own.
    pub fn of(&self, child: LineageKey) -> GenomeKey {
        self.keys
            .get(&child)
            .copied()
            .unwrap_or_else(|| GenomeKey::of_zygote(child))
    }
}

/// What the extraction counted, for the report (design note, section 3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PoolReport {
    /// Per locus of the frequencies, the pool persons with an alternative allele there.
    pub carriers: Vec<(DataId, u64)>,
    pub households: u64,
    pub persons: u64,
    pub unborn: u64,
    pub records: u64,
}

/// Why the extraction failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractError {
    /// A locus of the frequencies is not in the frozen map.
    UnknownLocus(DataId),
    /// A frequency is not in (0, 1).
    Frequency(DataId),
    /// The threshold disease recomputed differs from the pool run's affliction.
    Threshold(LineageKey),
    /// A locus of the frequencies on Y: no rule of section 3 assigns it.
    Chromosome(DataId),
}

/// A person the assignment visits: living, or an unborn child of a running pregnancy.
#[derive(Clone, Copy)]
struct Assigned {
    key: LineageKey,
    genome_key: GenomeKey,
    sex: Sex,
    genome: Genome,
    /// The living parents: their keys. A dead or emigrated parent is `None`.
    mother: Option<LineageKey>,
    father: Option<LineageKey>,
}

/// The allele copies at `locus` (design note, section 3).
fn assign_locus(
    draws: &Draws,
    person: &Assigned,
    locus: &crate::data::Locus,
    q: f64,
    assigned: &BTreeMap<LineageKey, Genome>,
) -> Vec<u8> {
    let draw = |detail: u64| {
        draws.unit(DrawKey {
            tag: tags::POOL_ALLELE,
            subject: SubjectKey::from(person.genome_key),
            occasion: str_key(&locus.id),
            detail,
        })
    };
    let parent = |key: Option<LineageKey>| key.and_then(|key| assigned.get(&key));
    // A copy from a parent with two copies: its maternal one if `u < 0.5`, else its
    // paternal one; from a missing parent the allele 1 if `u < q`.
    let from_two = |of: Option<&Genome>, detail: u64| {
        let u = draw(detail);
        match of {
            Some(genome) => {
                let copies = genome.copies(locus);
                if u < 0.5 { copies[0] } else { copies[1] }
            }
            None => u8::from(u < q),
        }
    };
    let mother = parent(person.mother);
    let father = parent(person.father);
    match locus.chromosome {
        Chromosome::Autosome => vec![from_two(mother, 0), from_two(father, 1)],
        Chromosome::X => {
            let maternal = from_two(mother, 0);
            match person.sex {
                Sex::Male => vec![maternal],
                Sex::Female => {
                    let paternal = match father {
                        Some(genome) => genome.copies(locus)[0],
                        None => u8::from(draw(1) < q),
                    };
                    vec![maternal, paternal]
                }
            }
        }
        Chromosome::Mt => match mother {
            Some(genome) => vec![genome.copies(locus)[0]],
            None => vec![u8::from(draw(0) < q)],
        },
        Chromosome::Y => panic!("invariant pool.y_locus"),
    }
}

/// The afflictions as at a creation (ADR-0017 section 3), without the exposure.
fn afflictions_of(
    data: &Data,
    pool_seed: u64,
    key: LineageKey,
    parents: Option<phenotype::Parents>,
    sex: Sex,
    genome: &Genome,
) -> Vec<PoolAffliction> {
    state::disease::afflictions(
        data,
        state::Seeds::one(pool_seed),
        (key, parents),
        sex,
        genome,
        false,
        SimTime::from_secs(0),
    )
    .iter()
    .map(|affliction| PoolAffliction {
        disease: affliction.disease,
        onset: affliction.onset,
    })
    .collect()
}

/// The pool of `world` at its last closed second (design note of the pool file).
/// `frozen` is the data of the frozen map with the seven loci and the diseases;
/// `frequencies` the `q` of each of the seven loci.
pub fn extract(
    world: &World,
    keys: &GenomeKeys,
    frozen: &Data,
    frequencies: &BTreeMap<DataId, f64>,
    pool_seed: u64,
) -> Result<(Pool, PoolReport), ExtractError> {
    let domain = world.state().domain();
    let at = world.closed_through();
    let mut loci = Vec::new();
    for (id, q) in frequencies {
        let locus = frozen
            .locus(id)
            .ok_or_else(|| ExtractError::UnknownLocus(id.clone()))?;
        if !(*q > 0.0 && *q < 1.0) {
            return Err(ExtractError::Frequency(id.clone()));
        }
        if locus.chromosome == Chromosome::Y {
            return Err(ExtractError::Chromosome(id.clone()));
        }
        loci.push((locus, *q));
    }
    let key_of = |sim: SimId| -> LineageKey {
        domain
            .sims
            .get(&sim)
            .map(|s| s.lineage_key)
            .or_else(|| domain.archive.get(&sim).map(|a| a.lineage_key))
            .unwrap_or_else(|| panic!("invariant pool.unknown_sim"))
    };
    let living = |sim: SimId| domain.sims.contains_key(&sim).then(|| key_of(sim));
    let pending = |sim: SimId, slot: Slot| {
        domain
            .pending
            .get(&(sim, slot))
            .and_then(|handle| world.pending_time(*handle))
    };

    // Section 3: the order, then the assignment.
    let mut people: Vec<(SimTime, Assigned)> = domain
        .sims
        .values()
        .map(|sim| {
            let parents = sim.parents.map(|p| (living(p.mother), living(p.father)));
            (
                sim.born,
                Assigned {
                    key: sim.lineage_key,
                    genome_key: keys.of(sim.lineage_key),
                    sex: sim.sex,
                    genome: sim.genome,
                    mother: parents.and_then(|p| p.0),
                    father: parents.and_then(|p| p.1),
                },
            )
        })
        .collect();
    people.sort_by_key(|(born, person)| (*born, person.key));
    let mut unborn: Vec<(SimTime, Assigned)> = Vec::new();
    for (mother, pregnancy) in &domain.pregnancies {
        let due =
            pending(*mother, Slot::Birth).unwrap_or_else(|| panic!("invariant pool.birth_pending"));
        for child in &pregnancy.children {
            unborn.push((
                due,
                Assigned {
                    key: child.lineage_key,
                    genome_key: keys.of(child.lineage_key),
                    sex: child.sex,
                    genome: child.genome,
                    mother: living(*mother),
                    father: pregnancy.father.and_then(living),
                },
            ));
        }
    }
    unborn.sort_by_key(|(due, person)| (*due, person.key));
    let draws = Draws::new(WorldSeed::new(pool_seed));
    let mut assigned: BTreeMap<LineageKey, Genome> = BTreeMap::new();
    for (_, person) in people.iter_mut().chain(unborn.iter_mut()) {
        let mut genome = person.genome;
        for (locus, q) in &loci {
            let copies = assign_locus(&draws, person, locus, *q, &assigned);
            genome.set_copies(locus, &copies);
        }
        person.genome = genome;
        assigned.insert(person.key, genome);
    }

    // Section 2: the pool households and their members.
    let mut members: BTreeMap<crate::ids::HouseholdId, Vec<SimId>> = BTreeMap::new();
    for (id, sim) in &domain.sims {
        members.entry(sim.household).or_default().push(*id);
    }
    let parents_of = |sim: SimId| -> Option<phenotype::Parents> {
        let parents = domain
            .sims
            .get(&sim)
            .and_then(|s| s.parents)
            .or_else(|| domain.archive.get(&sim).and_then(|a| a.parents))?;
        Some(phenotype::Parents {
            mother: key_of(parents.mother),
            father: key_of(parents.father),
        })
    };
    let run_diseases: BTreeSet<u64> = world
        .data()
        .diseases()
        .iter()
        .map(|disease| str_key(&disease.code))
        .collect();
    let mut report = PoolReport::default();
    let mut households = Vec::new();
    let mut pool_people: Vec<SimId> = Vec::new();
    let mut pool_unborn: Vec<LineageKey> = Vec::new();
    for (household, ids) in &members {
        let open = domain
            .households
            .get(household)
            .is_some_and(|h| h.ended.is_none());
        let sims: Vec<(SimId, &state::Sim)> =
            ids.iter().map(|id| (*id, &domain.sims[id])).collect();
        if !open || !sims.iter().any(|(_, sim)| adult(sim.born, at)) {
            continue;
        }
        let mut sims = sims;
        sims.sort_by_key(|(_, sim)| (sim.born, sim.lineage_key));
        let mut people_of = Vec::new();
        for (id, sim) in &sims {
            let genome = assigned[&sim.lineage_key];
            let afflictions = afflictions_of(
                frozen,
                pool_seed,
                sim.lineage_key,
                parents_of(*id),
                sim.sex,
                &genome,
            );
            let kept: Vec<(u64, FiniteF64)> = sim
                .afflictions
                .iter()
                .map(|a| (a.disease, a.onset))
                .collect();
            let recomputed: Vec<(u64, FiniteF64)> = afflictions
                .iter()
                .filter(|a| run_diseases.contains(&a.disease))
                .map(|a| (a.disease, a.onset))
                .collect();
            if kept != recomputed {
                return Err(ExtractError::Threshold(sim.lineage_key));
            }
            people_of.push(PoolPerson {
                lineage_key: sim.lineage_key,
                genome_key: keys.of(sim.lineage_key),
                sex: sim.sex,
                genome,
                born: sim.born,
                mother: sim.parents.map(|p| key_of(p.mother)),
                father: sim.parents.map(|p| key_of(p.father)),
                conceptions: sim.conceptions,
                births: sim.births,
                partnerships: sim.partnerships,
                moved_out: sim.moved_out,
                carer: sim.carer.map(key_of),
                social_parents: sim.social_parents.iter().map(|s| key_of(*s)).collect(),
                afflictions,
                conception_at: pending(*id, Slot::Conception),
                separation_at: pending(*id, Slot::Separation),
                leave_home_at: pending(*id, Slot::LeaveHome),
                birth_at: pending(*id, Slot::Birth),
            });
            pool_people.push(*id);
        }
        let in_household = |sim: &SimId| sims.iter().any(|(id, _)| id == sim);
        let partnership = sims.iter().find_map(|(id, _)| {
            let partnership = domain.partnerships.get(domain.open.get(id)?)?;
            (in_household(&partnership.woman) && in_household(&partnership.man)).then(|| {
                PoolPartnership {
                    woman: key_of(partnership.woman),
                    man: key_of(partnership.man),
                    formed: partnership.formed,
                }
            })
        });
        let mut pregnancies = Vec::new();
        for (id, sim) in &sims {
            let Some(pregnancy) = domain.pregnancies.get(id) else {
                continue;
            };
            let father = key_of(
                pregnancy
                    .father
                    .unwrap_or_else(|| panic!("invariant pool.pregnancy_father")),
            );
            let children = pregnancy
                .children
                .iter()
                .map(|child| {
                    let genome = assigned[&child.lineage_key];
                    let parents = phenotype::Parents {
                        mother: sim.lineage_key,
                        father,
                    };
                    pool_unborn.push(child.lineage_key);
                    PoolUnborn {
                        lineage_key: child.lineage_key,
                        genome_key: keys.of(child.lineage_key),
                        sex: child.sex,
                        genome,
                        afflictions: afflictions_of(
                            frozen,
                            pool_seed,
                            child.lineage_key,
                            Some(parents),
                            child.sex,
                            &genome,
                        ),
                    }
                })
                .collect();
            pregnancies.push(PoolPregnancy {
                mother: sim.lineage_key,
                father,
                conception: pregnancy.conception,
                conceived: pregnancy.conceived,
                children,
            });
        }
        report.persons += u64::try_from(people_of.len()).unwrap_or(u64::MAX);
        households.push(PoolHousehold {
            members: people_of,
            partnership,
            pregnancies,
        });
    }
    report.households = u64::try_from(households.len()).unwrap_or(u64::MAX);
    report.unborn = u64::try_from(pool_unborn.len()).unwrap_or(u64::MAX);

    // Section 6: the ancestors.
    let ancestors = ancestors(domain, &households, &pool_people, &key_of);
    report.records = u64::try_from(ancestors.len()).unwrap_or(u64::MAX);
    for (locus, _) in &loci {
        let genomes = households.iter().flat_map(|h| {
            h.members.iter().map(|m| m.genome).chain(
                h.pregnancies
                    .iter()
                    .flat_map(|p| p.children.iter().map(|c| c.genome)),
            )
        });
        let carriers = genomes.filter(|genome| genome.dose(locus) > 0).count();
        report.carriers.push((
            locus.id.clone(),
            u64::try_from(carriers).unwrap_or(u64::MAX),
        ));
    }
    let pool = Pool {
        pool_seed,
        frozen_at: at,
        data_hash: world.data().hash(),
        frozen_hash: frozen.hash(),
        households,
        ancestors,
    };
    Ok((pool, report))
}

/// The records of section 6: every pool person and its ancestors up to depth 3; an unborn
/// child is no record, its mother and father and their ancestors up to depth 2 are.
fn ancestors(
    domain: &DomainState,
    households: &[PoolHousehold],
    pool_people: &[SimId],
    key_of: &impl Fn(SimId) -> LineageKey,
) -> Vec<Ancestor> {
    let parents_of = |sim: SimId| {
        domain
            .sims
            .get(&sim)
            .and_then(|s| s.parents)
            .or_else(|| domain.archive.get(&sim).and_then(|a| a.parents))
            .map(|p| (p.mother, p.father))
    };
    let born_of = |sim: SimId| {
        domain
            .sims
            .get(&sim)
            .map(|s| s.born)
            .or_else(|| domain.archive.get(&sim).map(|a| a.born))
            .unwrap_or_else(|| panic!("invariant pool.unknown_sim"))
    };
    let mut records: BTreeSet<SimId> = BTreeSet::new();
    // (person, how many generations above it still count)
    let mut walk: Vec<(SimId, u32)> = pool_people.iter().map(|sim| (*sim, 3)).collect();
    for (mother, pregnancy) in &domain.pregnancies {
        let in_pool = households
            .iter()
            .any(|h| h.pregnancies.iter().any(|p| p.mother == key_of(*mother)));
        if in_pool {
            walk.push((*mother, 2));
            if let Some(father) = pregnancy.father {
                walk.push((father, 2));
            }
        }
    }
    while let Some((sim, depth)) = walk.pop() {
        records.insert(sim);
        if depth == 0 {
            continue;
        }
        if let Some((mother, father)) = parents_of(sim) {
            walk.push((mother, depth - 1));
            walk.push((father, depth - 1));
        }
    }
    let keys: BTreeMap<SimId, LineageKey> = records.iter().map(|s| (*s, key_of(*s))).collect();
    let named: BTreeSet<LineageKey> = keys.values().copied().collect();
    let mut list: Vec<Ancestor> = records
        .iter()
        .map(|sim| {
            let parents = parents_of(*sim);
            let parent = |p: Option<SimId>| p.map(key_of).filter(|key| named.contains(key));
            Ancestor {
                lineage_key: keys[sim],
                born: born_of(*sim),
                mother: parent(parents.map(|p| p.0)),
                father: parent(parents.map(|p| p.1)),
            }
        })
        .collect();
    list.sort_by_key(|record| record.lineage_key);
    list
}

/// Where one genome sits in the pool, so the extension can sort first and write after.
enum ExtensionPlace {
    Member(usize, usize),
    Unborn(usize, usize, usize),
}

/// Living members by `(born, lineage_key)`, then unborn children by the mother's pending
/// birth and the child's lineage key. A pregnancy whose mother has no `birth_at` breaks
/// the pool invariant `pool.birth_pending`.
fn extension_visits(pool: &Pool) -> Vec<(LineageKey, ExtensionPlace)> {
    let mut due_of = BTreeMap::new();
    for household in &pool.households {
        for member in &household.members {
            if let Some(at) = member.birth_at {
                due_of.insert(member.lineage_key, at);
            }
        }
    }
    let mut visits = Vec::new();
    for (h, household) in pool.households.iter().enumerate() {
        for (m, member) in household.members.iter().enumerate() {
            visits.push((
                0u8,
                member.born,
                member.lineage_key,
                ExtensionPlace::Member(h, m),
            ));
        }
        for (p, pregnancy) in household.pregnancies.iter().enumerate() {
            let due = due_of
                .get(&pregnancy.mother)
                .copied()
                .unwrap_or_else(|| panic!("invariant pool.birth_pending"));
            for (c, child) in pregnancy.children.iter().enumerate() {
                visits.push((1, due, child.lineage_key, ExtensionPlace::Unborn(h, p, c)));
            }
        }
    }
    visits.sort_by_key(|(kind, time, key, _)| (*kind, *time, *key));
    visits
        .into_iter()
        .map(|(_, _, key, place)| (key, place))
        .collect()
}

/// Writes `loci` into every pool genome. Only autosomal loci are written; any other
/// chromosome in the slice is skipped. A living member who has already been visited
/// transmits one copy through `assign_locus` at `q = 0.5`. A parent who is not a living
/// member, or who has not been visited yet, is missing, and that copy is drawn
/// independently at `p = 0.5`. Slots the slice does not name stay as they are. The same
/// draws address a genome key, so identical twins stay identical, and a second call
/// writes the same alleles.
pub fn extend_temperament(pool: &mut Pool, loci: &[crate::data::Locus]) {
    let loci: Vec<&crate::data::Locus> = loci
        .iter()
        .filter(|locus| locus.chromosome == Chromosome::Autosome)
        .collect();
    if loci.is_empty() {
        return;
    }
    let living: BTreeSet<LineageKey> = pool
        .households
        .iter()
        .flat_map(|household| household.members.iter().map(|member| member.lineage_key))
        .collect();
    let draws = Draws::new(WorldSeed::new(pool.pool_seed));
    let mut assigned: BTreeMap<LineageKey, Genome> = BTreeMap::new();
    let mut by_genome_key: BTreeMap<GenomeKey, Genome> = BTreeMap::new();
    for (_, place) in extension_visits(pool) {
        let (key, genome_key, sex, genome, mother, father) = match place {
            ExtensionPlace::Member(h, m) => {
                let member = &pool.households[h].members[m];
                (
                    member.lineage_key,
                    member.genome_key,
                    member.sex,
                    member.genome,
                    member.mother,
                    member.father,
                )
            }
            ExtensionPlace::Unborn(h, p, c) => {
                let pregnancy = &pool.households[h].pregnancies[p];
                let child = &pregnancy.children[c];
                (
                    child.lineage_key,
                    child.genome_key,
                    child.sex,
                    child.genome,
                    Some(pregnancy.mother),
                    Some(pregnancy.father),
                )
            }
        };
        let person = Assigned {
            key,
            genome_key,
            sex,
            genome,
            mother: mother.filter(|parent| living.contains(parent)),
            father: father.filter(|parent| living.contains(parent)),
        };
        let mut written = person.genome;
        // A genome key written before (an identical twin, whatever the order of birth
        // seconds and keys) gives its new alleles unchanged.
        let first = by_genome_key.get(&genome_key).copied();
        for locus in &loci {
            let copies = match first {
                Some(twin) => twin.copies(locus),
                None => assign_locus(&draws, &person, locus, 0.5, &assigned),
            };
            written.set_copies(locus, &copies);
        }
        by_genome_key.entry(genome_key).or_insert(written);
        assigned.insert(key, written);
        match place {
            ExtensionPlace::Member(h, m) => pool.households[h].members[m].genome = written,
            ExtensionPlace::Unborn(h, p, c) => {
                pool.households[h].pregnancies[p].children[c].genome = written;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::HouseholdId;

    fn id(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    fn key(n: u64) -> LineageKey {
        LineageKey::founder(&DataId::new("test:records").expect("id"), n)
    }

    /// Sim `n`, born in second `-n` (parents before their children), with these parents.
    fn sim(n: u64, parents: Option<(u64, u64)>) -> state::Sim {
        state::Sim {
            born: SimTime::from_secs(-i64::try_from(n).expect("small")),
            lineage_key: key(n),
            sex: Sex::Female,
            genome: Genome::reference(Sex::Female),
            afflictions: Vec::new(),
            conceptions: 0,
            births: 0,
            partnerships: 0,
            household: HouseholdId::try_from(1).expect("valid"),
            moved_out: false,
            carer: None,
            parents: parents.map(|(mother, father)| state::Parents {
                mother: id(mother),
                father: id(father),
                conception: 1,
            }),
            social_parents: Vec::new(),
            pool: None,
        }
    }

    fn pregnancy(father: u64) -> state::Pregnancy {
        state::Pregnancy {
            father: Some(id(father)),
            pool_father: None,
            conception: 1,
            children: Vec::new(),
            conceived: SimTime::from_secs(0),
        }
    }

    /// Section 6 of the design note: a pool person's ancestors count up to depth 3, the
    /// father of her unborn child and his up to depth 2, and nobody of a pregnancy whose
    /// mother is not in the pool (mutants).
    ///
    /// Sim 1 is the pool person: parents 2 and 3; 2's are 4 and 5, 4's are 6 and 7, 6's
    /// are 8 and 9 (depth 4), 8's are 10 and 11 (depth 5); on her father's side 3's are 22
    /// and 23, 22's are 24 and 25, 24's are 26 and 27 (depth 4). She is pregnant by 12:
    /// his parents 13 and 14, 13's are 15 and 16, 15's are 17 and 18 (depth 3 above him).
    /// Sim 20, not in the pool, is pregnant by 21.
    #[test]
    fn the_records_reach_depth_three_and_two() {
        let mut domain = DomainState::new();
        let parents = [
            (1, Some((2, 3))),
            (2, Some((4, 5))),
            (3, Some((22, 23))),
            (4, Some((6, 7))),
            (5, None),
            (6, Some((8, 9))),
            (7, None),
            (8, Some((10, 11))),
            (9, None),
            (10, None),
            (11, None),
            (12, Some((13, 14))),
            (13, Some((15, 16))),
            (14, None),
            (15, Some((17, 18))),
            (16, None),
            (17, None),
            (18, None),
            (20, None),
            (21, None),
            (22, Some((24, 25))),
            (23, None),
            (24, Some((26, 27))),
            (25, None),
            (26, None),
            (27, None),
        ];
        for (n, of) in parents {
            domain.sims.insert(id(n), sim(n, of));
        }
        domain.pregnancies.insert(id(1), pregnancy(12));
        domain.pregnancies.insert(id(20), pregnancy(21));
        let households = [PoolHousehold {
            members: Vec::new(),
            partnership: None,
            pregnancies: vec![PoolPregnancy {
                mother: key(1),
                father: key(12),
                conception: 1,
                conceived: SimTime::from_secs(0),
                children: Vec::new(),
            }],
        }];
        let key_of = |sim: SimId| domain.sims[&sim].lineage_key;
        let records: BTreeSet<LineageKey> = ancestors(&domain, &households, &[id(1)], &key_of)
            .iter()
            .map(|record| record.lineage_key)
            .collect();
        let expected: BTreeSet<LineageKey> =
            [1, 2, 3, 4, 5, 6, 7, 12, 13, 14, 15, 16, 22, 23, 24, 25]
                .into_iter()
                .map(key)
                .collect();
        assert_eq!(records, expected);
    }

    use crate::data::{Action, Load, Locus, Mark, MutationDirection, Weights};
    use crate::finite::FiniteF64;
    use crate::hash::stable_hash64;
    use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed};

    /// The 40 temperament loci in the reference's order (trait id, then locus number).
    /// Segment and slot are the free slots the map rule leaves on map 4.
    fn temperament_loci() -> Vec<Locus> {
        [
            ("core:agreeableness_01", 0, 12),
            ("core:agreeableness_02", 1, 14),
            ("core:agreeableness_03", 2, 14),
            ("core:agreeableness_04", 3, 11),
            ("core:agreeableness_05", 4, 16),
            ("core:agreeableness_06", 5, 11),
            ("core:agreeableness_07", 6, 10),
            ("core:agreeableness_08", 7, 13),
            ("core:conscientiousness_01", 3, 13),
            ("core:conscientiousness_02", 4, 17),
            ("core:conscientiousness_03", 5, 12),
            ("core:conscientiousness_04", 6, 11),
            ("core:conscientiousness_05", 7, 14),
            ("core:conscientiousness_06", 0, 13),
            ("core:conscientiousness_07", 1, 15),
            ("core:conscientiousness_08", 2, 15),
            ("core:extraversion_01", 6, 12),
            ("core:extraversion_02", 7, 15),
            ("core:extraversion_03", 0, 14),
            ("core:extraversion_04", 1, 16),
            ("core:extraversion_05", 2, 16),
            ("core:extraversion_06", 3, 14),
            ("core:extraversion_07", 4, 18),
            ("core:extraversion_08", 5, 13),
            ("core:neuroticism_01", 1, 17),
            ("core:neuroticism_02", 2, 17),
            ("core:neuroticism_03", 3, 15),
            ("core:neuroticism_04", 4, 19),
            ("core:neuroticism_05", 5, 14),
            ("core:neuroticism_06", 6, 13),
            ("core:neuroticism_07", 7, 16),
            ("core:neuroticism_08", 0, 15),
            ("core:openness_01", 4, 20),
            ("core:openness_02", 5, 15),
            ("core:openness_03", 6, 14),
            ("core:openness_04", 7, 17),
            ("core:openness_05", 0, 16),
            ("core:openness_06", 1, 18),
            ("core:openness_07", 2, 18),
            ("core:openness_08", 3, 16),
        ]
        .into_iter()
        .map(|(id, segment, slot)| {
            let trait_id = id.rsplit_once('_').expect("locus number").0;
            Locus {
                id: DataId::new(id).expect("id"),
                chromosome: Chromosome::Autosome,
                segment,
                slot,
                action: Action::Additive(Weights {
                    effect: FiniteF64::try_new(1.0).expect("finite"),
                    d_ref: FiniteF64::try_new(1.0).expect("finite"),
                    loads: vec![Load {
                        target: DataId::new(trait_id).expect("id"),
                        weight: FiniteF64::try_new(1.0).expect("finite"),
                    }],
                }),
                mark: Mark::Small,
                mutation: MutationDirection::Both,
            }
        })
        .collect()
    }

    fn founder(index: u64) -> LineageKey {
        LineageKey::founder(&DataId::new("test:temperament").expect("id"), index)
    }

    fn blank_person(
        key: LineageKey,
        sex: Sex,
        born: i64,
        mother: Option<LineageKey>,
        father: Option<LineageKey>,
        genome_key: LineageKey,
        birth_at: Option<i64>,
    ) -> PoolPerson {
        PoolPerson {
            lineage_key: key,
            genome_key: GenomeKey::of_zygote(genome_key),
            sex,
            genome: Genome::reference(sex),
            born: SimTime::from_secs(born),
            mother,
            father,
            conceptions: 0,
            births: 0,
            partnerships: 0,
            moved_out: false,
            carer: None,
            social_parents: Vec::new(),
            afflictions: Vec::new(),
            conception_at: None,
            separation_at: None,
            leave_home_at: None,
            birth_at: birth_at.map(SimTime::from_secs),
        }
    }

    fn unborn(key: LineageKey, genome_key: LineageKey) -> PoolUnborn {
        PoolUnborn {
            lineage_key: key,
            genome_key: GenomeKey::of_zygote(genome_key),
            sex: Sex::Female,
            genome: Genome::reference(Sex::Female),
            afflictions: Vec::new(),
        }
    }

    /// The fixture of tools/pool_extension_reference.py: two founders, a child of both, a
    /// child whose father is not in the pool, identical twins, and an unborn pair that
    /// shares a genome key. The mother's autosomes carry a 1 on an old slot and on a new one.
    fn fixture() -> (Pool, [LineageKey; 9]) {
        let keys: Vec<LineageKey> = (0..9).map(founder).collect();
        let [
            mother,
            father,
            child,
            half,
            twin_a,
            twin_b,
            unborn_key,
            unborn_twin,
            absent,
        ] = keys[..]
        else {
            unreachable!("nine founders");
        };
        let due = 5_000_000;
        let mut mother_person = blank_person(mother, Sex::Female, 0, None, None, mother, Some(due));
        mother_person.genome.maternal.set(0, 0, 1);
        mother_person.genome.maternal.set(0, 12, 1);
        let members = vec![
            mother_person,
            blank_person(father, Sex::Male, 10, None, None, father, None),
            blank_person(
                child,
                Sex::Female,
                100,
                Some(mother),
                Some(father),
                child,
                None,
            ),
            blank_person(half, Sex::Male, 110, Some(mother), Some(absent), half, None),
            blank_person(
                twin_a,
                Sex::Female,
                200,
                Some(mother),
                Some(father),
                twin_a,
                None,
            ),
            blank_person(
                twin_b,
                Sex::Female,
                200,
                Some(mother),
                Some(father),
                twin_a,
                None,
            ),
        ];
        let pool = Pool {
            pool_seed: 0x5EED,
            frozen_at: SimTime::from_secs(0),
            data_hash: 0,
            frozen_hash: 0,
            households: vec![PoolHousehold {
                members,
                partnership: None,
                pregnancies: vec![PoolPregnancy {
                    mother,
                    father,
                    conception: 1,
                    conceived: SimTime::from_secs(0),
                    children: vec![
                        unborn(unborn_key, unborn_key),
                        unborn(unborn_twin, unborn_key),
                    ],
                }],
            }],
            ancestors: Vec::new(),
        };
        (
            pool,
            [
                mother,
                father,
                child,
                half,
                twin_a,
                twin_b,
                unborn_key,
                unborn_twin,
                absent,
            ],
        )
    }

    fn genome_of(pool: &Pool, key: LineageKey) -> Genome {
        for household in &pool.households {
            for member in &household.members {
                if member.lineage_key == key {
                    return member.genome;
                }
            }
            for pregnancy in &household.pregnancies {
                for child in &pregnancy.children {
                    if child.lineage_key == key {
                        return child.genome;
                    }
                }
            }
        }
        panic!("missing genome");
    }

    fn assert_alleles(genome: Genome, loci: &[Locus], bits: &[u8]) {
        let mut i = 0;
        for locus in loci {
            for bit in genome.copies(locus) {
                assert_eq!(bit, bits[i] - b'0');
                i += 1;
            }
        }
        assert_eq!(i, bits.len());
    }

    fn draw_unit(seed: u64, genome_key: GenomeKey, locus: &Locus, detail: u64) -> f64 {
        Draws::new(WorldSeed::new(seed)).unit(DrawKey {
            tag: tags::POOL_ALLELE,
            subject: SubjectKey::from(genome_key),
            occasion: str_key(&locus.id),
            detail,
        })
    }

    fn clear_loci(mut genome: Genome, loci: &[Locus]) -> Genome {
        for locus in loci {
            genome.set_copies(locus, &[0, 0]);
        }
        genome
    }

    fn mendelian_genomes(pool: &Pool) -> u64 {
        let living: BTreeSet<LineageKey> = pool
            .households
            .iter()
            .flat_map(|household| household.members.iter().map(|member| member.lineage_key))
            .collect();
        let mut assigned = BTreeSet::new();
        let mut count = 0;
        for (key, place) in extension_visits(pool) {
            let (mother, father) = match place {
                ExtensionPlace::Member(h, m) => {
                    let member = &pool.households[h].members[m];
                    (member.mother, member.father)
                }
                ExtensionPlace::Unborn(h, p, _) => {
                    let pregnancy = &pool.households[h].pregnancies[p];
                    (Some(pregnancy.mother), Some(pregnancy.father))
                }
            };
            let transmits = |parent: Option<LineageKey>| {
                parent.is_some_and(|parent| living.contains(&parent) && assigned.contains(&parent))
            };
            if transmits(mother) || transmits(father) {
                count += 1;
            }
            assigned.insert(key);
        }
        count
    }

    fn allele_checksum(pool: &Pool, loci: &[Locus]) -> u64 {
        let mut bytes = Vec::new();
        for (key, _) in extension_visits(pool) {
            let genome = genome_of(pool, key);
            for locus in loci {
                bytes.extend(genome.copies(locus));
            }
        }
        stable_hash64(&bytes)
    }

    fn child_shares(
        pool: &Pool,
        child: LineageKey,
        mother: LineageKey,
        father: LineageKey,
        loci: &[Locus],
    ) -> bool {
        let child_genome = genome_of(pool, child);
        let child_key = extension_visits(pool)
            .into_iter()
            .find_map(|(key, place)| {
                if key != child {
                    return None;
                }
                let genome_key = match place {
                    ExtensionPlace::Member(h, m) => pool.households[h].members[m].genome_key,
                    ExtensionPlace::Unborn(h, p, c) => {
                        pool.households[h].pregnancies[p].children[c].genome_key
                    }
                };
                Some(genome_key)
            })
            .expect("child");
        let mother_genome = genome_of(pool, mother);
        let father_genome = genome_of(pool, father);
        loci.iter().all(|locus| {
            let copies = child_genome.copies(locus);
            let u0 = draw_unit(pool.pool_seed, child_key, locus, 0);
            let u1 = draw_unit(pool.pool_seed, child_key, locus, 1);
            let from = |genome: Genome, u: f64| {
                let parent = genome.copies(locus);
                if u < 0.5 { parent[0] } else { parent[1] }
            };
            copies[0] == from(mother_genome, u0) && copies[1] == from(father_genome, u1)
        })
    }

    #[test]
    fn the_temperament_extension_matches_the_fixture() {
        assert_eq!(TEMPERAMENT_POOL_EXTENSION, 1);
        let loci = temperament_loci();
        let (mut pool, keys) = fixture();
        let [
            mother,
            father,
            child,
            half,
            twin_a,
            twin_b,
            unborn_key,
            unborn_twin,
            absent,
        ] = keys;
        assert_eq!(mother.get(), 0xd719_6d66_1927_2530);
        assert_eq!(father.get(), 0x7409_46c5_6a17_38ab);
        assert_eq!(child.get(), 0x0e13_bf73_a6e1_0635);
        assert_eq!(half.get(), 0x14d2_3e17_9e9e_f8f6);
        assert_eq!(twin_a.get(), 0x5764_d8fd_67ad_23a2);
        assert_eq!(twin_b.get(), 0xfc5c_7d74_cd8a_4eb7);
        assert_eq!(unborn_key.get(), 0xf293_6405_a524_4b28);
        assert_eq!(unborn_twin.get(), 0x6e0c_6db4_f88a_57e2);
        assert_eq!(absent.get(), 0x4162_142f_d46c_25be);
        let before = pool.clone();
        extend_temperament(&mut pool, &loci);
        let once = pool.clone();
        extend_temperament(&mut pool, &loci);
        assert_eq!(pool, once);
        let expected = [
            (
                mother,
                b"01001010100111011111100111010001111010000111111111111101111111101100000000001100",
            ),
            (
                father,
                b"11001010101100000101001100010000011101011111011111111100001001010011010010010100",
            ),
            (
                child,
                b"11001100001110101110000110000000101100001111111111111100101110001001000000001000",
            ),
            (
                half,
                b"10001001011111001010101111110011100000000010111011101110101011101001000101001100",
            ),
            (
                twin_a,
                b"01000010110110001010100110100010111111001111101111111110101110011001010000011100",
            ),
            (
                twin_b,
                b"01000010110110001010100110100010111111001111101111111110101110011001010000011100",
            ),
            (
                unborn_key,
                b"11001001000110001111000110000010110110001111111111111100101110111001010001011100",
            ),
            (
                unborn_twin,
                b"11001001000110001111000110000010110110001111111111111100101110111001010001011100",
            ),
        ];
        for (key, bits) in expected {
            assert_alleles(genome_of(&pool, key), &loci, bits);
        }
        assert_eq!(allele_checksum(&pool, &loci), 0xf215_2566_5def_1e5f);
        assert_eq!(mendelian_genomes(&pool), 6);
        assert_eq!(extension_visits(&pool).len(), 8);
        assert_eq!(genome_of(&pool, mother).maternal.allele(0, 0), 1);
        assert_eq!(genome_of(&pool, mother).copies(&loci[0]), vec![0, 1]);
        for (key, _) in extension_visits(&pool) {
            assert_eq!(
                clear_loci(genome_of(&before, key), &loci),
                clear_loci(genome_of(&pool, key), &loci)
            );
        }
        assert_eq!(genome_of(&pool, twin_a), genome_of(&pool, twin_b));
        assert_eq!(genome_of(&pool, unborn_key), genome_of(&pool, unborn_twin));
        assert!(child_shares(&pool, child, mother, father, &loci));
        let half_genome = genome_of(&pool, half);
        let half_key = pool.households[0].members[3].genome_key;
        assert!(loci.iter().all(|locus| {
            let u = draw_unit(pool.pool_seed, half_key, locus, 1);
            half_genome.copies(locus)[1] == u8::from(u < 0.5)
        }));
    }

    /// The mother born at the twins' second, so she may be visited between them: the
    /// twins still get the same new alleles (tools/pool_extension_reference.py, line
    /// `fixture_late_mother_twin_a`).
    #[test]
    fn twins_stay_equal_when_a_parent_shares_their_birth_second() {
        let loci = temperament_loci();
        let (mut pool, keys) = fixture();
        let [mother, _, _, _, twin_a, twin_b, _, _, _] = keys;
        pool.households[0].members[0].born = SimTime::from_secs(200);
        assert_eq!(pool.households[0].members[0].lineage_key, mother);
        extend_temperament(&mut pool, &loci);
        assert_alleles(
            genome_of(&pool, twin_a),
            &loci,
            b"11100010111100101000101100000000111111000101100101111100001100010001110000111100",
        );
        assert_eq!(genome_of(&pool, twin_a), genome_of(&pool, twin_b));
    }

    /// The repository pool file stays byte-identical. The extension's alleles, the
    /// Mendelian count and the hash of the extended pool are the reference's.
    #[test]
    fn the_repository_pool_extension_matches_the_reference() {
        let bytes = include_bytes!("../../../data/pool/pool.bin");
        assert_eq!(stable_hash64(bytes), 0x5cc9_dc3e_b6db_8524);
        let mut pool = Pool::from_bytes(bytes).expect("pool");
        assert_eq!(pool.hash(), 0x5cc9_dc3e_b6db_8524);
        let before = pool.clone();
        let loci = temperament_loci();
        extend_temperament(&mut pool, &loci);
        let once = pool.clone();
        extend_temperament(&mut pool, &loci);
        assert_eq!(pool, once);
        assert_eq!(pool.hash(), 0x12ec_62c5_7af2_a290);
        assert_eq!(allele_checksum(&pool, &loci), 0x7151_a1c8_95a7_3f91);
        assert_eq!(extension_visits(&pool).len(), 40_418);
        assert_eq!(mendelian_genomes(&pool), 29_942);
        let mut alt = 0u64;
        let mut copies = 0u64;
        for (key, _) in extension_visits(&pool) {
            let genome = genome_of(&pool, key);
            for locus in &loci {
                for bit in genome.copies(locus) {
                    alt += u64::from(bit);
                    copies += 1;
                }
            }
            assert_eq!(
                clear_loci(genome_of(&before, key), &loci),
                clear_loci(genome, &loci)
            );
        }
        assert_eq!(alt, 1_615_810);
        assert_eq!(copies, 3_233_440);
        // |alt / copies - 1/2| < 1/100, in integers: |2 alt - copies| * 50 < copies.
        let alt = u128::from(alt);
        let copies = u128::from(copies);
        assert!((alt * 2).abs_diff(copies) * 50 < copies);
        let mut seen = BTreeMap::new();
        for household in &pool.households {
            for member in &household.members {
                if let Some(previous) = seen.insert(member.genome_key, member.genome) {
                    assert_eq!(previous, member.genome);
                }
            }
            for pregnancy in &household.pregnancies {
                for child in &pregnancy.children {
                    if let Some(previous) = seen.insert(child.genome_key, child.genome) {
                        assert_eq!(previous, child.genome);
                    }
                }
            }
        }
        let living: BTreeSet<LineageKey> = pool
            .households
            .iter()
            .flat_map(|household| household.members.iter().map(|member| member.lineage_key))
            .collect();
        let shared = pool.households.iter().find_map(|household| {
            household.members.iter().find(|member| {
                member.mother.is_some_and(|mother| living.contains(&mother))
                    && member.father.is_some_and(|father| living.contains(&father))
            })
        });
        let member = shared.expect("a child of two living pool parents");
        assert!(child_shares(
            &pool,
            member.lineage_key,
            member.mother.expect("mother"),
            member.father.expect("father"),
            &loci
        ));
    }

    #[test]
    fn the_genome_map_places_the_temperament_loci_in_the_reference_slots() {
        let text = include_str!("../../../data/genetics/genome_map.json");
        let value: serde_json::Value = serde_json::from_str(text).expect("json");
        let loci = value["loci"].as_array().expect("loci");
        for spec in temperament_loci() {
            let locus = loci
                .iter()
                .find(|locus| locus["id"] == spec.id.as_str())
                .unwrap_or_else(|| panic!("missing {}", spec.id.as_str()));
            assert_eq!(locus["chromosome"], "autosome");
            assert_eq!(locus["segment"], spec.segment);
            assert_eq!(locus["slot"], spec.slot);
            assert_eq!(locus["mark"], "small");
            assert_eq!(locus["mutation"], "both");
            assert_eq!(locus["action"]["additive"]["effect"], 1.0);
            assert_eq!(locus["action"]["additive"]["d_ref"], 1.0);
            let loads = locus["action"]["additive"]["loads"]
                .as_array()
                .expect("loads");
            assert_eq!(loads.len(), 1);
            assert_eq!(loads[0]["weight"], 1.0);
        }
    }
}
