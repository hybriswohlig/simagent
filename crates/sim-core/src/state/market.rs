//! The partner market (genetics specification 11.1, 11.2 and 11.7, ADR-0018 sections 1 and
//! 2): a monthly round in which single women of the market age may search and each
//! searching woman, in the order of her rank, chooses among up to `k` single men of the
//! market age by a utility with Gumbel noise. Men do not search (R8). Relatives with
//! `φ ≥ 1/8` are skipped, nearer kin below that limit pays a penalty. The similarity terms
//! of cognition and height compare the phenotypes of the traits the data name (4.2); the
//! base map has none, so there they drop out (11.1).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::{DomainState, partnership};
use crate::data::Data;
use crate::data_id::Code;
use crate::genome::Sex;
use crate::ids::SimId;
use crate::kinship::{Kin, KinshipCache};
use crate::lineage::LineageKey;
use crate::log::{Cause, Causes, DrawRef};
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{SimTime, YEAR};
use crate::world::SimCtx;

/// The age from which a woman searches with the full rate: before it `ρ` scales her rate.
const ADULT_YEARS: i64 = 18;

/// Whether someone born at `born` is at least `years` old at `at`, counted in seconds from
/// the birthday on (11.1).
pub(crate) fn of_age(born: SimTime, at: SimTime, years: i64) -> bool {
    let Some(since) = at.checked_since(born) else {
        return false;
    };
    YEAR.checked_mul(years)
        .is_some_and(|years| since.as_secs() >= years.as_secs())
}

/// The market age in whole years (11.2, 16).
pub(crate) fn market_age(data: &Data) -> i64 {
    i64::from(data.demography().market.age_years)
}

/// The probability that a single woman born at `born` searches in a round at `at` (11.2):
/// `s_f(a) = min(1, c · e^(−½ · ((a − m) / w)²))`, until her 18th birthday times `ρ`.
pub(crate) fn search_chance(data: &Data, born: SimTime, at: SimTime) -> f64 {
    let market = data.demography().market;
    let a = super::disease::age(born, at);
    let z = (a - market.search_center) / market.search_width;
    let s = market.search_height * sim_math::exp((-0.5 * z) * z);
    let capped = if s < 1.0 { s } else { 1.0 };
    if of_age(born, at, ADULT_YEARS) {
        capped
    } else {
        market.minor_factor * capped
    }
}

/// The address of her search in the round `month`.
pub(crate) fn search_key(her: LineageKey, month: u64) -> DrawKey {
    DrawKey {
        tag: tags::MARKET_SEARCH,
        subject: SubjectKey::from(her),
        occasion: month,
        detail: 0,
    }
}

/// The address of the noise of the candidate `his` for her in the round `month`.
pub(crate) fn noise_key(her: LineageKey, month: u64, his: LineageKey) -> DrawKey {
    DrawKey {
        tag: tags::PAIRING_NOISE,
        subject: SubjectKey::from(her),
        occasion: month,
        detail: his.get(),
    }
}

/// The utility of a candidate (11.2): `U = −w_K · dK² − w_H · dH² − w_a · (Δa − μ_a)² −
/// 16 · ln(odds) · φ + G`, the penalties subtracted in this order; `differences` holds `dK`
/// and `dH`, each `None` where the data name no trait, and then the term is left out.
fn utility(data: &Data, differences: [Option<f64>; 2], gap: f64, phi: f64, noise: f64) -> f64 {
    let market = data.demography().market;
    let d = gap - market.age_gap;
    let kin = 16.0 * sim_math::ln(market.cousin_odds);
    let similarity = [market.weight_cognition, market.weight_height]
        .into_iter()
        .zip(differences)
        .filter_map(|(weight, difference)| difference.map(|x| weight * (x * x)));
    let mut penalties = similarity.chain([market.weight_age * (d * d), kin * phi]);
    let first = penalties
        .next()
        .unwrap_or_else(|| panic!("invariant market.utility"));
    let u = penalties.fold(-first, |sum, penalty| sum - penalty) + noise;
    if !u.is_finite() {
        panic!("invariant market.utility");
    }
    u
}

/// The phenotypes the market compares, cognition and the target height (4.2, 11.2), of a
/// living sim at `at`; each `None` where the data name no such trait.
fn compared(
    domain: &DomainState,
    data: &Data,
    draws: &Draws,
    sim: SimId,
    at: SimTime,
) -> [Option<f64>; 2] {
    let person = super::conception::person(domain, sim);
    let age = at
        .checked_since(person.born)
        .unwrap_or_else(|| panic!("invariant market.age"));
    data.pairing_traits().map(|named| {
        named.map(|t| {
            crate::phenotype::phenotype(
                draws,
                data,
                person.genome,
                person.lineage_key,
                person.parents,
                t,
                age,
            )
            .unwrap_or_else(|| panic!("invariant market.phenotype"))
        })
    })
}

/// Birth and parents of a person of the living or the archive, for φ.
fn kin_of(domain: &DomainState, sim: SimId) -> Kin {
    let (born, parents) = domain
        .sims
        .get(&sim)
        .map(|person| (person.born, person.parents))
        .or_else(|| {
            domain
                .archive
                .get(&sim)
                .map(|person| (person.born, person.parents))
        })
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()));
    Kin {
        born,
        parents: parents.map(|p| (p.mother, p.father)),
    }
}

/// φ of two persons with the world's cache (ADR-0018 section 2).
pub(crate) fn phi(domain: &mut DomainState, x: SimId, y: SimId) -> f64 {
    let mut cache = core::mem::take(&mut domain.kinship);
    let value = phi_with(&mut cache, domain, x, y);
    domain.kinship = cache;
    value
}

/// φ of two persons with the given cache.
pub(crate) fn phi_with(cache: &mut KinshipCache, domain: &DomainState, x: SimId, y: SimId) -> f64 {
    cache.phi(x, y, |sim| kin_of(domain, sim))
}

/// A round of the market at the month start `now` (11.2, ADR-0018 section 1).
pub(crate) fn on_round(domain: &mut DomainState, data: &Data, ctx: &mut SimCtx<'_, '_>) {
    let now = ctx.now();
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant market.before_start"));
    let month = super::conception::month_index(now)
        .unwrap_or_else(|| panic!("invariant market.before_start"));
    let market = data.demography().market;
    let draws = Draws::new(WorldSeed::new(seed));
    let age = market_age(data);
    let single =
        |domain: &DomainState, sim: SimId| partnership::open_partnership(domain, sim).is_none();
    // 1. The single men of the market age, fixed for the round.
    let men: BTreeMap<LineageKey, SimId> = domain
        .sims
        .iter()
        .filter(|(id, person)| {
            person.sex == Sex::Male && of_age(person.born, now, age) && single(domain, **id)
        })
        .map(|(id, person)| (person.lineage_key, *id))
        .collect();
    // 2. The single women of the market age who search.
    let searching: BTreeMap<LineageKey, SimId> = domain
        .sims
        .iter()
        .filter(|(id, person)| {
            person.sex == Sex::Female
                && of_age(person.born, now, age)
                && single(domain, **id)
                && draws.bernoulli(
                    search_key(person.lineage_key, month),
                    search_chance(data, person.born, now),
                )
        })
        .map(|(id, person)| (person.lineage_key, *id))
        .collect();
    // 3. They choose in the order of their rank.
    let order = draws.ranked(
        tags::MARKET_ORDER,
        SubjectKey::WORLD,
        month,
        searching.keys().copied(),
    );
    let limit = usize::try_from(market.candidates).unwrap_or(usize::MAX);
    for her_key in order {
        let woman = searching[&her_key];
        // 4. Her candidates: the men by rank, relatives at or above the limit skipped.
        let ranked = draws.ranked(
            tags::PAIRING_RANK,
            SubjectKey::from(her_key),
            month,
            men.keys().copied(),
        );
        let mut candidates: Vec<(SimId, LineageKey, f64)> = Vec::new();
        for his_key in ranked {
            if candidates.len() == limit {
                break;
            }
            let man = men[&his_key];
            let kinship = phi(domain, woman, man);
            if kinship >= market.kinship_limit {
                continue;
            }
            candidates.push((man, his_key, kinship));
        }
        // 5. and 6. The largest utility; on a tie the earlier in the list.
        let her_age = super::disease::age(born_of(domain, woman), now);
        let hers = compared(domain, data, &draws, woman, now);
        let mut best: Option<(f64, SimId, LineageKey)> = None;
        for (man, his_key, kinship) in candidates {
            let gap = super::disease::age(born_of(domain, man), now) - her_age;
            let his = compared(domain, data, &draws, man, now);
            let differences = [0, 1].map(|n| hers[n].zip(his[n]).map(|(a, b)| a - b));
            let noise = draws.gumbel(noise_key(her_key, month, his_key));
            let u = utility(data, differences, gap, kinship, noise);
            if better(u, best.map(|(top, _, _)| top)) {
                best = Some((u, man, his_key));
            }
        }
        let Some((_, man, his_key)) = best else {
            continue;
        };
        // No second try: a man chosen earlier in this round is no longer single.
        if !single(domain, man) {
            continue;
        }
        let rule = Code::new("core:partner_market")
            .unwrap_or_else(|_| panic!("invariant market.rule_code"));
        let causes = Causes::new(ctx.trigger())
            .with(Cause::Rule(rule))
            .with(Cause::Random(DrawRef::from(noise_key(
                her_key, month, his_key,
            ))));
        partnership::pair_up_with(domain, data, (woman, man), causes, ctx);
    }
}

/// Whether a candidate with the utility `u` beats the best so far (`None` before the
/// first): only a larger utility does, so on a tie the earlier candidate, the one with the
/// smaller rank, stays (11.2).
fn better(u: f64, best: Option<f64>) -> bool {
    best.is_none_or(|top| u > top)
}

/// The second of birth of a living sim.
fn born_of(domain: &DomainState, sim: SimId) -> SimTime {
    domain
        .sims
        .get(&sim)
        .map(|person| person.born)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "From 16 on" is from the 16th birthday on, in seconds (11.1): a year has
    /// `YEAR` seconds.
    #[test]
    fn the_market_age_begins_on_the_birthday() {
        let born = SimTime::from_secs(-5);
        let birthday = SimTime::from_secs(16 * YEAR.as_secs() - 5);
        assert!(!of_age(
            born,
            SimTime::from_secs(birthday.as_secs() - 1),
            16
        ));
        assert!(of_age(born, birthday, 16));
        assert!(!of_age(SimTime::from_secs(10), SimTime::from_secs(5), 0));
    }

    /// The first candidate is the best so far; a later one only with a larger utility, so
    /// on a tie the smaller rank wins (11.2).
    #[test]
    fn a_tie_keeps_the_earlier_candidate() {
        assert!(better(-3.0, None));
        assert!(better(0.5, Some(0.25)));
        assert!(!better(0.25, Some(0.25)));
        assert!(!better(0.0, Some(0.25)));
    }

    /// The search chance and the utility on the repository's data, bit for bit as
    /// `tools/world_reference.py` computes them (11.2): for a woman born at 0, `ρ` until
    /// her 18th birthday; the utility without the similarity terms.
    #[test]
    fn the_values_of_the_python_reference() {
        let data = crate::data::testing::data();
        let born = SimTime::START;
        let chance = |secs: i64| search_chance(&data, born, SimTime::from_secs(secs));
        assert_eq!(chance(503_193_600), 0.000_453_751_052_916_396_54);
        assert_eq!(chance(550_368_000), 0.000_588_243_244_093_722_9);
        assert_eq!(chance(566_061_350), 0.000_634_806_044_370_002_6);
        assert_eq!(chance(566_092_799), 0.000_634_899_722_251_718_8);
        assert_eq!(chance(566_092_800), 0.006_916_119_011_225_014);
        assert_eq!(chance(793_473_408), 0.011_79);
        assert_eq!(chance(1_257_984_000), 0.001_272_777_747_205_707_1);
        assert_eq!(chance(1_886_976_000), 5.175_433_469_609_017_6e-8);
        assert_eq!(utility(&data, [None, None], 4.69, 0.0, 0.0), 0.0);
        assert_eq!(
            utility(&data, [None, None], 0.0, 0.0, 1.5),
            1.060_077_999_999_999_9
        );
        assert_eq!(
            utility(&data, [None, None], 3.0, 0.0625, 0.0),
            -2.359_707_092_994_046
        );
        assert_eq!(
            utility(&data, [None, None], 10.0, 0.031_25, -2.0),
            -3.715_214_546_497_022_8
        );
    }
}
