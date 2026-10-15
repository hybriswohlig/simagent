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
use crate::time::{SimTime, YEAR};
use crate::world::World;

/// The format of the pool file.
pub const POOL_FORMAT: u64 = 1;

/// The first bytes of a pool file.
pub const MAGIC: &[u8; 7] = b"SIMPOOL";

/// The pool of a pool run (design note of the pool file, section 7).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pool {
    pub pool_seed: u64,
    /// `E_p`, the last second of the pool run.
    pub frozen_at: SimTime,
    /// The hash of the pool run's data.
    pub data_hash: u64,
    /// In the order of their `HouseholdId` in the pool run.
    pub households: Vec<PoolHousehold>,
    /// Ascending by lineage key.
    pub ancestors: Vec<Ancestor>,
}

/// A household of the pool: its members ascending by birth, then lineage key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PoolHousehold {
    pub members: Vec<PoolPerson>,
    pub partnership: Option<PoolPartnership>,
    /// By the place of the mother among the members.
    pub pregnancies: Vec<PoolPregnancy>,
}

/// A living person of the pool (design note, section 5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
        Ok(())
    }
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
        pool_seed,
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
                    father: living(pregnancy.father),
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
    let mut pool_unborn: Vec<(SimId, LineageKey)> = Vec::new();
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
            let father = key_of(pregnancy.father);
            let children = pregnancy
                .children
                .iter()
                .map(|child| {
                    let genome = assigned[&child.lineage_key];
                    let parents = phenotype::Parents {
                        mother: sim.lineage_key,
                        father,
                    };
                    pool_unborn.push((pregnancy.father, child.lineage_key));
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
            walk.push((pregnancy.father, 2));
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
        }
    }

    fn pregnancy(father: u64) -> state::Pregnancy {
        state::Pregnancy {
            father: id(father),
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
}
