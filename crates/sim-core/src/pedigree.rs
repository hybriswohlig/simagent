//! The pedigree of a world (B10, HANDOFF §9 step 10): every sim, living or archived, with
//! its biological and social parents, its household, its carer and its hereditary diseases;
//! every partnership with its end and the kinship of the two; every household. A read-only
//! view; whoever shows it computes nothing domain-relevant from it.

use alloc::vec::Vec;

use serde::Serialize;

use crate::data_id::Code;
use crate::genome::{LocusAlleles, Sex};
use crate::ids::{HouseholdId, PartnershipId, SimId};
use crate::input::{Input, InputRecord};
use crate::kinship::KinshipCache;
use crate::lineage::{GenomeKey, LineageKey};
use crate::params::ParamChange;
use crate::state::{Affliction, EndedBy, LeftBy, market};
use crate::time::SimTime;

/// The version of the form of `pedigree.json` (step 10 added social parents, households,
/// carers, diseases, the end of a partnership and its kinship; step 11 the emigration).
pub const PEDIGREE_FORMAT: u32 = 4;

/// All sims, all partnerships and all households, each by ID.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pedigree {
    pub format_version: u32,
    pub persons: Vec<Person>,
    pub partnerships: Vec<Couple>,
    pub households: Vec<Home>,
}

/// A sim, living or archived. Founders have no parents.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Person {
    pub sim: SimId,
    pub lineage_key: LineageKey,
    pub sex: Sex,
    /// The loci with an alternative allele, in map order (the form of a founder).
    pub alleles: Vec<LocusAlleles>,
    pub born: SimTime,
    /// The second of the death, `None` while the sim lives or after it emigrated.
    pub died: Option<SimTime>,
    pub cause_of_death: Option<Code>,
    /// The second it left the town with its household (ADR-0020 section 6), else `None`.
    pub emigrated: Option<SimTime>,
    pub mother: Option<SimId>,
    pub father: Option<SimId>,
    /// Who took it in by the orphan rule, in the order of the state (ADR-0018 section 4).
    pub social_parents: Vec<SimId>,
    /// The household it lives in; for the dead the last one.
    pub household: HouseholdId,
    pub moved_out: bool,
    /// Its carer while it is dependent; `None` for the dead and the emigrated.
    pub carer: Option<SimId>,
    /// Its hereditary diseases in the order of the data (ADR-0017 section 3).
    pub diseases: Vec<Shown>,
    /// Where a pool person came from (ADR-0020 section 5); `None` for a town person.
    pub pool: Option<FromPool>,
}

/// A pool person's origin (design note of the arrival, section 8): its relations as
/// lineage keys, since its parents may never have come into the town.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FromPool {
    pub genome_key: GenomeKey,
    /// Its birth second in the pool run; `None` for a child of a pool pregnancy born in
    /// the town.
    pub born_pool: Option<SimTime>,
    pub arrived: SimTime,
    pub arrival_year: u64,
    pub mother_key: Option<LineageKey>,
    pub father_key: Option<LineageKey>,
    pub social_parent_keys: Vec<LineageKey>,
}

impl FromPool {
    fn of(origin: &crate::state::PoolOrigin) -> FromPool {
        FromPool {
            genome_key: origin.genome_key,
            born_pool: origin.born_pool,
            arrived: origin.arrived,
            arrival_year: origin.arrival_year,
            mother_key: origin.mother,
            father_key: origin.father,
            social_parent_keys: origin.social_parents.clone(),
        }
    }
}

/// A hereditary disease of a person: its onset age in years and whether it shows.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Shown {
    pub disease: Code,
    pub onset: f64,
    /// The onset is reached at the reference second and the switch of the hereditary
    /// diseases is on then (ADR-0017 section 3).
    pub affected: bool,
}

/// A partnership with its forming, its end and the kinship of the two.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Couple {
    pub partnership: PartnershipId,
    pub woman: SimId,
    pub man: SimId,
    pub formed: SimTime,
    pub ended: Option<SimTime>,
    pub ended_by: Option<End>,
    /// φ of the two (genetics specification 11.2).
    pub kinship: f64,
}

/// Why a partnership ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum End {
    Death,
    Separation,
    Emigration,
}

/// A household from its forming to its end; it stays as history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Home {
    pub household: HouseholdId,
    pub formed: SimTime,
    pub ended: Option<SimTime>,
}

/// Whether the switch of the hereditary diseases is on at `at`: the value of `Start` from
/// the start on, each override block's from one second after its stamp (ADR-0013).
pub(crate) fn switch_at(log: &[InputRecord], at: SimTime) -> bool {
    let mut on = false;
    for record in log {
        match &record.input {
            Input::Start(start) => on = start.world_params.hereditary_diseases,
            Input::ForkOverride(block) if record.t.as_secs() < at.as_secs() => {
                for change in &block.changes {
                    if let ParamChange::HereditaryDiseases(value) = change {
                        on = *value;
                    }
                }
            }
            _ => {}
        }
    }
    on
}

impl crate::world::World {
    /// The pedigree as the state holds it: the persons by ID, the living and the archive
    /// together, the partnerships and the households by ID.
    pub fn pedigree(&self) -> Pedigree {
        let state = self.state();
        let domain = state.domain();
        let data = state.data();
        let log = self.input_log();
        // Senescence, accident or a disease of the data (ADR-0017 section 2).
        let code = |key: u64| crate::state::death::cause_code(data, key);
        let shown = |afflictions: &[Affliction], born: SimTime, at: SimTime| -> Vec<Shown> {
            let on = switch_at(log, at);
            afflictions
                .iter()
                .filter_map(|affliction| {
                    let onset = affliction.onset.get();
                    let age = crate::state::disease::age(born, at);
                    Some(Shown {
                        disease: code(affliction.disease)?,
                        onset,
                        affected: on && age >= onset,
                    })
                })
                .collect()
        };
        // The living show their diseases at the clock, right after `Start` at the start.
        let now = self.closed_through().max(SimTime::START);
        let living = domain.sims.iter().map(|(id, sim)| Person {
            sim: *id,
            lineage_key: sim.lineage_key,
            sex: sim.sex,
            alleles: sim.genome.alleles(data),
            born: sim.born,
            died: None,
            cause_of_death: None,
            emigrated: None,
            mother: sim.parents.map(|parents| parents.mother),
            father: sim.parents.map(|parents| parents.father),
            social_parents: sim.social_parents.clone(),
            household: sim.household,
            moved_out: sim.moved_out,
            carer: sim.carer,
            diseases: shown(&sim.afflictions, sim.born, now),
            pool: sim.pool.as_ref().map(FromPool::of),
        });
        let archived = domain.archive.iter().map(|(id, sim)| {
            let (died, cause_of_death, emigrated) = match sim.left_by {
                LeftBy::Death { cause } => (Some(sim.left), code(cause), None),
                LeftBy::Emigration => (None, None, Some(sim.left)),
            };
            Person {
                sim: *id,
                lineage_key: sim.lineage_key,
                sex: sim.sex,
                alleles: sim.genome.alleles(data),
                born: sim.born,
                died,
                cause_of_death,
                emigrated,
                mother: sim.parents.map(|parents| parents.mother),
                father: sim.parents.map(|parents| parents.father),
                social_parents: sim.social_parents.clone(),
                household: sim.household,
                moved_out: sim.moved_out,
                carer: None,
                diseases: shown(&sim.afflictions, sim.born, sim.left),
                pool: sim.pool.as_ref().map(FromPool::of),
            }
        });
        let mut persons: Vec<Person> = living.chain(archived).collect();
        persons.sort_unstable_by_key(|person| person.sim);
        let mut cache = KinshipCache::new();
        let partnerships = domain
            .partnerships
            .iter()
            .map(|(id, partnership)| Couple {
                partnership: *id,
                woman: partnership.woman,
                man: partnership.man,
                formed: partnership.formed,
                ended: partnership.ended.map(|ending| ending.at),
                ended_by: partnership.ended.map(|ending| match ending.by {
                    EndedBy::Death => End::Death,
                    EndedBy::Separation => End::Separation,
                    EndedBy::Emigration => End::Emigration,
                }),
                kinship: market::phi_with(
                    &mut cache,
                    domain,
                    data,
                    partnership.woman,
                    partnership.man,
                ),
            })
            .collect();
        let households = domain
            .households
            .iter()
            .map(|(id, household)| Home {
                household: *id,
                formed: household.formed,
                ended: household.ended,
            })
            .collect();
        Pedigree {
            format_version: PEDIGREE_FORMAT,
            persons,
            partnerships,
            households,
        }
    }
}

impl crate::world::WorldState {
    /// φ of two persons, living or archived (genetics specification 11.2, the function
    /// the market uses); `None` if either is unknown.
    pub fn kinship(&self, a: SimId, b: SimId) -> Option<f64> {
        let domain = self.domain();
        let known = |sim: &SimId| domain.sims.contains_key(sim) || domain.archive.contains_key(sim);
        (known(&a) && known(&b))
            .then(|| market::phi_with(&mut KinshipCache::new(), domain, self.data(), a, b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_id::DataId;
    use crate::ids::InputSeq;
    use crate::input::{ForkOverride, Start};
    use crate::params::WorldParams;

    fn record(t: i64, input: Input) -> InputRecord {
        InputRecord {
            t: SimTime::from_secs(t),
            seq: InputSeq::try_from(1).expect("valid"),
            input,
        }
    }

    fn start(on: bool) -> Input {
        Input::Start(Start {
            world_seed: 1,
            founder_tag: DataId::new("test:switch").expect("id"),
            world_params: WorldParams {
                hereditary_diseases: on,
                nutrition: Vec::new(),
            },
            founders: Vec::new(),
            couples: Vec::new(),
            capacity: None,
            pool: None,
        })
    }

    fn set(on: bool) -> Input {
        Input::ForkOverride(ForkOverride {
            changes: alloc::vec![ParamChange::HereditaryDiseases(on)],
        })
    }

    /// `Start` sets the switch from the start on; an override block from one second after
    /// its stamp on; within a block the last change counts.
    #[test]
    fn the_switch_follows_the_input_log() {
        let t = SimTime::from_secs;
        let log = [record(-1, start(true)), record(10, set(false))];
        assert!(switch_at(&log, t(0)));
        assert!(switch_at(&log, t(10)));
        assert!(!switch_at(&log, t(11)));
        let twice = Input::ForkOverride(ForkOverride {
            changes: alloc::vec![
                ParamChange::HereditaryDiseases(true),
                ParamChange::HereditaryDiseases(false),
            ],
        });
        let log = [
            record(-1, start(false)),
            record(3, twice),
            record(7, set(true)),
        ];
        assert!(!switch_at(&log, t(4)));
        assert!(!switch_at(&log, t(7)));
        assert!(switch_at(&log, t(8)));
        assert!(!switch_at(&[], t(8)));
    }

    /// A world rule (ADR-0023 section 2) sets the switch from one second after its stamp
    /// on, like an override block; an arranged partnership leaves it.
    #[test]
    fn the_switch_follows_the_world_rules() {
        let t = SimTime::from_secs;
        let rule = |on| {
            Input::SetWorldParam(crate::input::SetWorldParam {
                change: ParamChange::HereditaryDiseases(on),
            })
        };
        let pair = Input::ArrangePartnership(crate::input::ArrangePartnership {
            woman: crate::ids::SimId::try_from(2).expect("valid"),
            man: crate::ids::SimId::try_from(1).expect("valid"),
        });
        let log = [
            record(-1, start(false)),
            record(5, rule(true)),
            record(6, pair),
            record(20, rule(false)),
            record(30, set(true)),
        ];
        assert!(!switch_at(&log, t(5)));
        assert!(switch_at(&log, t(6)));
        assert!(switch_at(&log, t(20)));
        assert!(!switch_at(&log, t(21)));
        assert!(!switch_at(&log, t(30)));
        assert!(switch_at(&log, t(31)));
    }
}
