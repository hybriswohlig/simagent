//! The pedigree of a world (B10, HANDOFF §9 step 5: a mini pedigree in the export): every
//! sim, living or archived, with its biological parents, and every partnership. A read
//! only view; whoever shows it computes nothing domain-relevant from it.

use alloc::vec::Vec;

use serde::Serialize;

use crate::data_id::Code;
use crate::genome::{LocusAlleles, Sex};
use crate::ids::{PartnershipId, SimId};
use crate::lineage::LineageKey;
use crate::state::LeftBy;
use crate::time::SimTime;

/// All sims by ID and all partnerships by ID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pedigree {
    pub persons: Vec<Person>,
    pub partnerships: Vec<Couple>,
}

/// A sim, living or archived. Founders have no parents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Person {
    pub sim: SimId,
    pub lineage_key: LineageKey,
    pub sex: Sex,
    /// The loci with an alternative allele, in map order (the form of a founder).
    pub alleles: Vec<LocusAlleles>,
    pub born: SimTime,
    /// The second of the death, `None` while the sim lives.
    pub died: Option<SimTime>,
    pub cause_of_death: Option<Code>,
    pub mother: Option<SimId>,
    pub father: Option<SimId>,
}

/// A partnership with its forming and its end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Couple {
    pub partnership: PartnershipId,
    pub woman: SimId,
    pub man: SimId,
    pub formed: SimTime,
    pub ended: Option<SimTime>,
}

impl crate::world::WorldState {
    /// The pedigree as the state holds it: the persons by ID, the living and the archive
    /// together, and the partnerships by ID.
    pub fn pedigree(&self) -> Pedigree {
        let domain = self.domain();
        let living = domain.sims.iter().map(|(id, sim)| Person {
            sim: *id,
            lineage_key: sim.lineage_key,
            sex: sim.sex,
            alleles: sim.genome.alleles(self.data()),
            born: sim.born,
            died: None,
            cause_of_death: None,
            mother: sim.parents.map(|parents| parents.mother),
            father: sim.parents.map(|parents| parents.father),
        });
        // A cause of `Start`, or a disease of the data (ADR-0017 section 2).
        let cause_of = |key: u64| {
            domain
                .death_causes
                .iter()
                .find(|cause| cause.cause.key() == key)
                .map(|cause| cause.cause.clone())
                .or_else(|| {
                    crate::state::disease::of_key(self.data(), key)
                        .map(|disease| crate::data_id::Code::from(disease.code.clone()))
                })
        };
        let archived = domain.archive.iter().map(|(id, sim)| {
            let LeftBy::Death { cause } = sim.left_by;
            Person {
                sim: *id,
                lineage_key: sim.lineage_key,
                sex: sim.sex,
                alleles: sim.genome.alleles(self.data()),
                born: sim.born,
                died: Some(sim.left),
                cause_of_death: cause_of(cause),
                mother: sim.parents.map(|parents| parents.mother),
                father: sim.parents.map(|parents| parents.father),
            }
        });
        let mut persons: Vec<Person> = living.chain(archived).collect();
        persons.sort_unstable_by_key(|person| person.sim);
        let partnerships = domain
            .partnerships
            .iter()
            .map(|(id, partnership)| Couple {
                partnership: *id,
                woman: partnership.woman,
                man: partnership.man,
                formed: partnership.formed,
                ended: partnership.ended,
            })
            .collect();
        Pedigree {
            persons,
            partnerships,
        }
    }
}
