//! Kinship words (ADR-0021 section 3, Grok's G6 and K1 to K6): what B is to A, as a code
//! of the core beside `r = 2φ`. The word comes from the nearest common ancestor over
//! biological parents only ([`relation`], the persons and paths a query of φ reads) and the
//! sex of B; a social parent gets a word of its own only for the direct edge, beside the
//! biological word. Partners are no kinship words (K6). Read only, like the rest of the
//! presentation state.
//!
//! # Places
//!
//! The nearest common ancestor stands at up, down ≤ 4 and not both at 4, or at one of
//! `(5, 1)`, `(1, 5)`, `(1, 6)` and `(2, 5)`. A shortest path from A up through the set of
//! the query has at most three steps among A's own ancestors to depth 3 (each of them lies
//! within three steps); a fourth step reaches a person of B's set, an ancestor of B within
//! three, so a common ancestor `Q` at `(≤ 4, ≤ 3)`, or `(≤ 3, ≤ 4)` from B's side. The
//! nearest one, the minimum by `(up + down, up)`, has a sum of at most 7. It is five or more
//! up only with a sum below `Q`'s: `(5, 1)` (B itself is never five up: the fourth person
//! on the path would be an ancestor and a descendant of B at once, and the same holds for A
//! five down). It is five or more down with a sum below `Q`'s or, by fewer up, equal to it:
//! `(1, 5)`, `(1, 6)` and `(2, 5)`. Those four need a loop (a parent who is also a
//! great-grandparent, or the like). Every place has a word, and any other one would be
//! "entfernt verwandt" (K4, never reached); the path to that ancestor alone gives
//! `φ ≥ 1/256`, so no `r` above 0 rounds to 0.0 (K5).

use serde::Serialize;

use crate::genome::Sex;
use crate::ids::SimId;
use crate::kinship::{Relation, relation};
use crate::state::DomainState;
use crate::state::market::kin_of;
use crate::world::World;

/// What B is to A by blood (Grok's K1 and K2): the direct line up to four generations, the
/// siblings (half only here), the siblings of ancestors and the children of siblings with
/// "Groß" and then "Ur", and cousins by their grade and the generations between them.
/// A code is never renumbered (ADR-0004).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KinWord {
    #[serde(rename = "self")]
    Myself,
    Mother,
    Father,
    Grandmother,
    Grandfather,
    GreatGrandmother,
    GreatGrandfather,
    GreatGreatGrandmother,
    GreatGreatGrandfather,
    Daughter,
    Son,
    Granddaughter,
    Grandson,
    GreatGranddaughter,
    GreatGrandson,
    GreatGreatGranddaughter,
    GreatGreatGrandson,
    Sister,
    Brother,
    HalfSister,
    HalfBrother,
    Aunt,
    Uncle,
    GreatAunt,
    GreatUncle,
    GreatGreatAunt,
    GreatGreatUncle,
    GreatGreatGreatAunt,
    GreatGreatGreatUncle,
    Niece,
    Nephew,
    GreatNiece,
    GreatNephew,
    GreatGreatNiece,
    GreatGreatNephew,
    GreatGreatGreatNiece,
    GreatGreatGreatNephew,
    GreatGreatGreatGreatNiece,
    GreatGreatGreatGreatNephew,
    FirstCousinFemale,
    FirstCousinMale,
    FirstCousinFemaleOlder1,
    FirstCousinMaleOlder1,
    FirstCousinFemaleOlder2,
    FirstCousinMaleOlder2,
    FirstCousinFemaleYounger1,
    FirstCousinMaleYounger1,
    FirstCousinFemaleYounger2,
    FirstCousinMaleYounger2,
    FirstCousinFemaleYounger3,
    FirstCousinMaleYounger3,
    SecondCousinFemale,
    SecondCousinMale,
    SecondCousinFemaleOlder1,
    SecondCousinMaleOlder1,
    SecondCousinFemaleYounger1,
    SecondCousinMaleYounger1,
    /// A place without a word of its own (K4); no search reaches one (see Places).
    Distant,
    Unrelated,
}

/// What B is to A by the direct edge of the orphan rule (Grok's K3): B took A in, or A took
/// B in. A code is never renumbered (ADR-0004).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Foster {
    FosterMother,
    FosterFather,
    FosterDaughter,
    FosterSon,
}

/// What the viewer shows of B on A's page.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct KinView {
    pub word: KinWord,
    pub foster: Option<Foster>,
    /// `r = 2φ` in percent to a tenth ([`r_percent`]).
    pub r_percent: f64,
}

/// The word of the place of the nearest common ancestor and the sex of B: `up` generations
/// above A, `down` from there to B (Grok's K1). Cousins are of the grade `min(up, down) − 1`;
/// B is older by `up − down` generations when it stands higher, younger by `down − up`.
/// Half only for siblings (K2). Any other place is distant (K4); no search reaches one.
pub fn word(place: Option<Relation>, sex: Sex) -> KinWord {
    use KinWord::*;
    let Some(Relation { up, down, shared }) = place else {
        return Unrelated;
    };
    let pick = |female: KinWord, male: KinWord| match sex {
        Sex::Female => female,
        Sex::Male => male,
    };
    match (up, down) {
        (0, 0) => Myself,
        (1, 0) => pick(Mother, Father),
        (2, 0) => pick(Grandmother, Grandfather),
        (3, 0) => pick(GreatGrandmother, GreatGrandfather),
        (4, 0) => pick(GreatGreatGrandmother, GreatGreatGrandfather),
        (0, 1) => pick(Daughter, Son),
        (0, 2) => pick(Granddaughter, Grandson),
        (0, 3) => pick(GreatGranddaughter, GreatGrandson),
        (0, 4) => pick(GreatGreatGranddaughter, GreatGreatGrandson),
        (1, 1) if shared == 1 => pick(HalfSister, HalfBrother),
        (1, 1) => pick(Sister, Brother),
        (2, 1) => pick(Aunt, Uncle),
        (3, 1) => pick(GreatAunt, GreatUncle),
        (4, 1) => pick(GreatGreatAunt, GreatGreatUncle),
        (5, 1) => pick(GreatGreatGreatAunt, GreatGreatGreatUncle),
        (1, 2) => pick(Niece, Nephew),
        (1, 3) => pick(GreatNiece, GreatNephew),
        (1, 4) => pick(GreatGreatNiece, GreatGreatNephew),
        (1, 5) => pick(GreatGreatGreatNiece, GreatGreatGreatNephew),
        (1, 6) => pick(GreatGreatGreatGreatNiece, GreatGreatGreatGreatNephew),
        (2, 2) => pick(FirstCousinFemale, FirstCousinMale),
        (3, 2) => pick(FirstCousinFemaleOlder1, FirstCousinMaleOlder1),
        (4, 2) => pick(FirstCousinFemaleOlder2, FirstCousinMaleOlder2),
        (2, 3) => pick(FirstCousinFemaleYounger1, FirstCousinMaleYounger1),
        (2, 4) => pick(FirstCousinFemaleYounger2, FirstCousinMaleYounger2),
        (2, 5) => pick(FirstCousinFemaleYounger3, FirstCousinMaleYounger3),
        (3, 3) => pick(SecondCousinFemale, SecondCousinMale),
        (4, 3) => pick(SecondCousinFemaleOlder1, SecondCousinMaleOlder1),
        (3, 4) => pick(SecondCousinFemaleYounger1, SecondCousinMaleYounger1),
        _ => Distant,
    }
}

/// The word of a direct social edge (Grok's K3), shown beside the blood word: `raised_by`
/// if B took A in (B is a social parent of A), `raised` if A took B in, and the sex of B.
/// None beside a mother, father, daughter or son: the orphan rule never makes a biological
/// parent a social one.
pub fn foster(blood: KinWord, raised_by: bool, raised: bool, sex: Sex) -> Option<Foster> {
    let pick = |female: Foster, male: Foster| match sex {
        Sex::Female => female,
        Sex::Male => male,
    };
    match blood {
        KinWord::Mother | KinWord::Father | KinWord::Daughter | KinWord::Son => None,
        _ if raised_by => Some(pick(Foster::FosterMother, Foster::FosterFather)),
        _ if raised => Some(pick(Foster::FosterDaughter, Foster::FosterSon)),
        _ => None,
    }
}

/// `2^31`: φ of a query is a multiple of `2^-31` in `[0, 1]` (the kinship module, Numbers).
const PHI_SCALE: i64 = 1 << 31;

/// `r = 2φ` in percent, rounded half up to a tenth (Grok's G6 and K5). With `φ = n / 2^31`
/// the tenths are `⌊(2000 n + 2^30) / 2^31⌋`, computed in whole numbers; the result is the
/// f64 nearest to that many tenths, as a decimal literal of it reads.
///
/// # Panics
///
/// Invariant `kinship.phi_form` for a φ that is no such multiple.
pub fn r_percent(phi: f64) -> f64 {
    let scale = sim_math::i64_to_f64_exact(PHI_SCALE).unwrap_or_else(|| panic!("2^31 is exact"));
    let n = sim_math::to_i64_exact(phi * scale)
        .filter(|n| (0..=PHI_SCALE).contains(n))
        .unwrap_or_else(|| panic!("invariant kinship.phi_form"));
    let tenths = (2000 * n + PHI_SCALE / 2).div_euclid(PHI_SCALE);
    sim_math::i64_to_f64_exact(tenths).unwrap_or_else(|| panic!("at most 2000 tenths")) / 10.0
}

/// Whether `carer` is a social parent of `child` (ADR-0018 section 4): taken in by the
/// orphan rule in the town, or in the pool run before both came into the town.
fn raised_by(domain: &DomainState, child: SimId, carer: SimId) -> bool {
    let (town, pool) = (domain.sims.get(&child))
        .map(|person| (&person.social_parents, person.pool.as_ref()))
        .or_else(|| {
            (domain.archive.get(&child))
                .map(|person| (&person.social_parents, person.pool.as_ref()))
        })
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", child.get()));
    town.contains(&carer)
        || pool.is_some_and(|from| from.social_parents.contains(&domain.key_of(carer)))
}

impl World {
    /// What `b` is to `a`, both living or archived; `None` if either is unknown.
    pub fn kin_view(&self, a: SimId, b: SimId) -> Option<KinView> {
        let state = self.state();
        let phi = state.kinship(a, b)?;
        let domain = state.domain();
        let sex = (domain.sims.get(&b).map(|person| person.sex))
            .or_else(|| domain.archive.get(&b).map(|person| person.sex))?;
        let place = relation(domain.key_of(a), domain.key_of(b), |key| {
            kin_of(domain, state.data(), key)
        });
        let blood = word(place, sex);
        Some(KinView {
            word: blood,
            foster: foster(blood, raised_by(domain, a, b), raised_by(domain, b, a), sex),
            r_percent: r_percent(phi),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_id::DataId;
    use crate::genome::Genome;
    use crate::ids::HouseholdId;
    use crate::lineage::{GenomeKey, LineageKey};
    use crate::state::{PoolOrigin, Sim};
    use crate::time::SimTime;
    use alloc::vec::Vec;

    fn sim(n: u64) -> SimId {
        SimId::try_from(n).expect("valid")
    }

    fn key(n: u64) -> LineageKey {
        LineageKey::founder(&DataId::new("test:kin").expect("valid"), n)
    }

    fn person(n: u64, social_parents: Vec<SimId>, pool: Option<PoolOrigin>) -> Sim {
        Sim {
            born: SimTime::from_secs(0),
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
            parents: None,
            social_parents,
            pool,
        }
    }

    /// The edge of the orphan rule in the town and the one a pool person brings: sim 2 was
    /// taken in by sim 1 in the town, sim 3 by sim 1 in the pool run; neither edge goes the
    /// other way, and sim 4 was taken in by nobody.
    #[test]
    fn a_social_parent_in_the_town_or_from_the_pool() {
        let origin = PoolOrigin {
            genome_key: GenomeKey::of_zygote(key(3)),
            born_pool: Some(SimTime::from_secs(0)),
            arrived: SimTime::from_secs(0),
            arrival_year: 0,
            mother: None,
            father: None,
            social_parents: Vec::from([key(1)]),
        };
        let mut domain = DomainState::new();
        domain.sims.insert(sim(1), person(1, Vec::new(), None));
        domain
            .sims
            .insert(sim(2), person(2, Vec::from([sim(1)]), None));
        domain
            .sims
            .insert(sim(3), person(3, Vec::new(), Some(origin)));
        domain.sims.insert(sim(4), person(4, Vec::new(), None));
        assert!(raised_by(&domain, sim(2), sim(1)));
        assert!(raised_by(&domain, sim(3), sim(1)));
        assert!(!raised_by(&domain, sim(1), sim(2)));
        assert!(!raised_by(&domain, sim(1), sim(3)));
        assert!(!raised_by(&domain, sim(4), sim(1)));
        assert!(!raised_by(&domain, sim(2), sim(3)));
    }
}
