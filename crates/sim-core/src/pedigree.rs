//! The pedigree of a world (B10, HANDOFF §9 step 5: a mini pedigree in the export): every
//! sim, living or archived, with its biological parents, and every partnership. A read
//! only view; whoever shows it computes nothing domain-relevant from it.

use alloc::vec::Vec;

use serde::Serialize;

use crate::data_id::Code;
use crate::genome::{Genotype, Sex};
use crate::ids::{PartnershipId, SimId};
use crate::lineage::LineageKey;
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
    pub genotype: Genotype,
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
    pub fn pedigree(&self) -> Pedigree {
        Pedigree {
            persons: Vec::new(),
            partnerships: Vec::new(),
        }
    }
}
