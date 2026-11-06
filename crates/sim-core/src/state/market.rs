//! The partner market (genetics specification 11.1, 11.2 and 11.7, ADR-0018 sections 1 and
//! 2): a monthly round in which single women of the market age may search and each
//! searching woman, in the order of her rank, chooses among up to `k` single men of the
//! market age by a utility with Gumbel noise. Men do not search (R8). Relatives with
//! `φ ≥ 1/8` are skipped, nearer kin below that limit pays a penalty. The similarity terms
//! of cognition and height compare the phenotypes of the traits the data name (4.2); the
//! base map has none, so there they drop out (11.1).
//!
//! With the data's group of six values (11.2a, ADR-0018 addendum of 2026-11-06) the chosen
//! man answers her one proposal by his own utility, and both have a target age moved by a
//! taste drawn once per person. Without the group he does not answer, nobody has a taste,
//! and the round and its draws are those of 11.2.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::conception::Lot;
use super::{DomainState, partnership};
use crate::data::Data;
use crate::data_id::Code;
use crate::demography::MaleAnswer;
use crate::genome::Sex;
use crate::ids::SimId;
use crate::kinship::{self, Kin, KinshipCache};
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

/// The market age in whole years (11.2).
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

/// The address of the Gumbel noise of the answer of the man `his` to the woman `her` in the
/// round `month` (11.2a, 11.7).
pub(crate) fn accept_key(his: LineageKey, month: u64, her: LineageKey) -> DrawKey {
    DrawKey {
        tag: tags::PAIRING_ACCEPT,
        subject: SubjectKey::from(his),
        occasion: month,
        detail: her.get(),
    }
}

/// The address of the taste of a person (11.2a, 11.7): its own key alone.
pub(crate) fn taste_key(person: LineageKey) -> DrawKey {
    DrawKey {
        tag: tags::PAIRING_TASTE,
        subject: SubjectKey::from(person),
        occasion: 0,
        detail: 0,
    }
}

/// The taste `ν ~ normal(0, σ_ν)` of a person (11.2a): a pure function of the seed of its lot
/// (`Lot::seed`) and its lineage key; it stands only in the target age.
pub(crate) fn taste(seed: u64, person: LineageKey, sd: f64) -> f64 {
    Draws::new(WorldSeed::new(seed)).normal(taste_key(person), 0.0, sd)
}

/// The target age of a man of `age` years, without his taste (11.2a): `max(a − δ(a), min)`
/// with `δ(a) = min(slope · max(a − from, 0), cap)`.
pub(crate) fn male_target(male: &MaleAnswer, age: f64) -> f64 {
    let since = if age > male.target_from {
        age - male.target_from
    } else {
        0.0
    };
    let slope = male.target_slope * since;
    let delta = if slope < male.target_cap {
        slope
    } else {
        male.target_cap
    };
    let target = age - delta;
    if target > male.target_min {
        target
    } else {
        male.target_min
    }
}

/// His utility for her proposal (11.2a): `U_m = −w_K · dK² − w_H · dH² − w_am · d² + G_m`,
/// `d` her age minus his target age (taste included); `differences` as in [`utility`], and a
/// trait the data do not name drops its term for him as for her (11.1). No kinship term.
fn male_utility(
    data: &Data,
    male: &MaleAnswer,
    differences: [Option<f64>; 2],
    distance: f64,
    noise: f64,
) -> f64 {
    let market = data.demography().market;
    let similarity = [market.weight_cognition, market.weight_height]
        .into_iter()
        .zip(differences)
        .filter_map(|(weight, difference)| difference.map(|x| weight * (x * x)));
    let mut penalties = similarity.chain([male.weight_age * (distance * distance)]);
    let first = penalties
        .next()
        .unwrap_or_else(|| panic!("invariant market.male_utility"));
    let u = penalties.fold(-first, |sum, penalty| sum - penalty) + noise;
    if !u.is_finite() {
        panic!("invariant market.male_utility");
    }
    u
}

/// Whether the partnership forms (11.2a): the chosen man must still be single, and with the
/// group of six values he answers, `U_m > 0` (strictly). Without it he does not answer. A
/// man who is taken is not asked, so `utility` (which draws his noise) runs only for a single
/// man.
fn accepted(
    single: bool,
    male: Option<&MaleAnswer>,
    utility: impl FnOnce(&MaleAnswer) -> f64,
) -> bool {
    single && male.is_none_or(|male| utility(male) > 0.0)
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
fn compared(domain: &DomainState, data: &Data, sim: SimId, at: SimTime) -> [Option<f64>; 2] {
    let person = super::conception::person(domain, data, sim);
    let age = at
        .checked_since(person.born)
        .unwrap_or_else(|| panic!("invariant market.age"));
    data.pairing_traits().map(|named| {
        named.map(|t| {
            person
                .phenotype(data, t, age)
                .unwrap_or_else(|| panic!("invariant market.phenotype"))
        })
    })
}

/// The classes of the order of φ (ADR-0020 section 4): everyone with a birth in the pool
/// run comes before everyone born in the town.
pub(crate) const POOL: u8 = 0;
pub(crate) const TOWN: u8 = 1;

/// Order and parents of a person for φ, by its lineage key (design note of the arrival,
/// section 7): a person of the town, living or archived (a pool person by its pool birth
/// and its pool keys), or a record of the data's pool.
///
/// # Panics
///
/// Invariant `kinship.unknown_key` for a key that is neither: every parent key of the pool
/// is a record (the pool's reader checks it).
pub(crate) fn kin_of(domain: &DomainState, data: &Data, key: LineageKey) -> Kin<LineageKey> {
    if let Some(&sim) = domain.keys.get(&key) {
        let (born, parents, pool) = domain
            .sims
            .get(&sim)
            .map(|person| (person.born, person.parents, person.pool.as_ref()))
            .or_else(|| {
                domain
                    .archive
                    .get(&sim)
                    .map(|person| (person.born, person.parents, person.pool.as_ref()))
            })
            .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()));
        return match pool {
            Some(from) => Kin {
                order: from.born_pool.map_or((TOWN, born), |at| (POOL, at)),
                mother: from.mother,
                father: from.father,
            },
            None => Kin {
                order: (TOWN, born),
                mother: parents.map(|p| domain.key_of(p.mother)),
                father: parents.map(|p| domain.key_of(p.father)),
            },
        };
    }
    let record = data
        .pool()
        .and_then(|held| held.record(key))
        .unwrap_or_else(|| panic!("invariant kinship.unknown_key"));
    Kin {
        order: (POOL, record.born),
        mother: record.mother,
        father: record.father,
    }
}

/// φ of two persons with the world's cache (ADR-0018 section 2).
pub(crate) fn phi(domain: &mut DomainState, data: &Data, x: SimId, y: SimId) -> f64 {
    let mut cache = core::mem::take(&mut domain.kinship);
    let value = phi_with(&mut cache, domain, data, x, y);
    domain.kinship = cache;
    value
}

/// φ of two persons of the town with the given cache, over their lineage keys.
pub(crate) fn phi_with(
    cache: &mut KinshipCache<LineageKey>,
    domain: &DomainState,
    data: &Data,
    x: SimId,
    y: SimId,
) -> f64 {
    let (kx, ky) = (domain.key_of(x), domain.key_of(y));
    cache.phi(kx, ky, |key| kin_of(domain, data, key))
}

/// Whether two persons of the town are in direct line, over their lineage keys and the same
/// parents as φ (ADR-0018 section 8).
pub(crate) fn direct_line(domain: &DomainState, data: &Data, x: SimId, y: SimId) -> bool {
    let (kx, ky) = (domain.key_of(x), domain.key_of(y));
    kinship::in_direct_line(kx, ky, |key| kin_of(domain, data, key))
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
        // 4. Her candidates: the men by rank, relatives at or above the limit and in direct
        // line skipped (ADR-0018 section 8).
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
            let kinship = phi(domain, data, woman, man);
            if kinship >= market.kinship_limit || direct_line(domain, data, woman, man) {
                continue;
            }
            candidates.push((man, his_key, kinship));
        }
        // 5. and 6. The largest utility; on a tie the earlier in the list. Her taste moves
        // her target age, `t_f = a_f + μ_a + ν`, so the gap she compares with `μ_a` is
        // `Δa − ν` (11.2a).
        let her_age = super::disease::age(born_of(domain, woman), now);
        let hers = compared(domain, data, woman, now);
        let nu = market.male.map_or(0.0, |male| {
            taste(Lot::of(domain, woman).seed(data), her_key, male.taste_sd)
        });
        let mut best: Option<(f64, SimId, LineageKey)> = None;
        for (man, his_key, kinship) in candidates {
            let gap = super::disease::age(born_of(domain, man), now) - her_age - nu;
            let his = compared(domain, data, man, now);
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
        // 7. No second try: a man chosen earlier in this round is no longer single and is
        // not asked. With the group he answers (11.2a); a refusal ends her round, it is no
        // state and no event.
        let answers = |male: &MaleAnswer| {
            let born = born_of(domain, man);
            let his_age = super::disease::age(born, now);
            let target = male_target(male, his_age)
                + taste(Lot::of(domain, man).seed(data), his_key, male.taste_sd);
            let his = compared(domain, data, man, now);
            let differences = [0, 1].map(|n| hers[n].zip(his[n]).map(|(a, b)| a - b));
            let noise = draws.gumbel(accept_key(his_key, month, her_key));
            male_utility(data, male, differences, her_age - target, noise)
        };
        if !accepted(single(domain, man), market.male.as_ref(), answers) {
            continue;
        }
        let rule = Code::new("core:partner_market")
            .unwrap_or_else(|_| panic!("invariant market.rule_code"));
        let mut causes = Causes::new(ctx.trigger())
            .with(Cause::Rule(rule))
            .with(Cause::Random(DrawRef::from(noise_key(
                her_key, month, his_key,
            ))));
        if market.male.is_some() {
            causes = causes.with(Cause::Random(DrawRef::from(accept_key(
                his_key, month, her_key,
            ))));
        }
        partnership::pair_and_move(domain, data, (woman, man), causes, ctx);
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

    /// The spec values of the group (11.2a): slope 0.4, from 22, cap 15, minimum 21, his
    /// weight 0.02, taste 3.
    fn male() -> MaleAnswer {
        MaleAnswer {
            taste_sd: 3.0,
            target_slope: 0.4,
            target_from: 22.0,
            target_cap: 15.0,
            target_min: 21.0,
            weight_age: 0.02,
        }
    }

    fn near(x: f64, y: f64) -> bool {
        (x - y).abs() <= 1e-12
    }

    /// His target age without taste, by hand from 11.2a: `δ = min(0.4 · max(a − 22, 0), 15)`,
    /// `t = max(a − δ, 21)`. 16 and 21: δ = 0, so the minimum 21. 25: δ = 1.2, 23.8 [W11.2d].
    /// 30: δ = 3.2, 26.8. 40: δ = 7.2, 32.8 [W11.2e]. 50: δ = 11.2, 38.8. 60: δ = 15.2 held at
    /// the cap 15, 45 [W11.2f]. 70: δ = 15, 55. Ages are continuous: 21.5 is before the
    /// slope (δ = 0) and above the minimum, 59.5 is where the slope meets the cap.
    #[test]
    fn the_mans_target_age_rises_with_his_age() {
        for (age, target) in [
            (16.0, 21.0),
            (21.0, 21.0),
            (21.5, 21.5),
            (22.0, 22.0),
            (25.0, 23.8),
            (30.0, 26.8),
            (40.0, 32.8),
            (50.0, 38.8),
            (59.5, 44.5),
            (60.0, 45.0),
            (70.0, 55.0),
        ] {
            assert!(
                near(male_target(&male(), age), target),
                "{age}: {}",
                male_target(&male(), age)
            );
        }
    }

    /// The taste is added after the minimum: a boy of 16 with `ν` = −4 aims at 21 − 4 = 17,
    /// a man of 40 with `ν` = 2.5 at 32.8 + 2.5 = 35.3.
    #[test]
    fn the_taste_moves_the_target_after_the_minimum() {
        assert!(near(male_target(&male(), 16.0) + -4.0, 17.0));
        assert!(near(male_target(&male(), 40.0) + 2.5, 35.3));
    }

    /// `U_m = −w_K · dz_K² − w_H · dz_h² − w_am · (a_f − t_m)² + G_m` with `w_K` = 0.377 and
    /// `w_H` = 0.164 (the repository's fitted weights) and `w_am` = 0.02. She is 20 and he aims
    /// at 23.8, so `d` = −3.8 and the age term is 0.02 · 14.44 = 0.2888. With `dz_K` = 1 and
    /// `dz_h` = 2 the traits cost 0.377 + 0.164 · 4 = 1.033, together 1.3218.
    #[test]
    fn the_man_accepts_when_his_utility_is_above_zero() {
        let data = crate::data::testing::data();
        let u = |differences, noise| male_utility(&data, &male(), differences, -3.8, noise);
        let both = [Some(1.0), Some(2.0)];
        assert!(near(u(both, 2.0), 0.6782));
        assert!(near(u(both, 1.3), -0.0218));
        // A trait the data do not name drops its term (11.1): height only, 0.164 · 4 + 0.2888.
        assert!(near(u([None, Some(2.0)], 1.0), 1.0 - 0.656 - 0.2888));
        // No trait at all: the age term alone.
        assert!(near(u([None, None], 0.29), 0.29 - 0.2888));
        assert!(near(u([None, None], 0.28), 0.28 - 0.2888));
        assert!(u(both, 2.0) > 0.0 && u(both, 1.3) <= 0.0);
        assert!(u([None, None], 0.29) > 0.0 && u([None, None], 0.28) <= 0.0);
    }

    /// Without the group the chosen man does not answer: he forms the partnership if he is
    /// single, and nothing is drawn. With it, only a man who is still single is asked: a taken
    /// man is neither asked nor drawn for, and the answer is strict, `U_m > 0`.
    #[test]
    fn only_a_single_man_answers_and_only_by_a_utility_above_zero() {
        use core::cell::Cell;
        let male = male();
        let asked = Cell::new(0_u32);
        let utility = |u: f64| {
            let asked = &asked;
            move |_: &MaleAnswer| {
                asked.set(asked.get() + 1);
                u
            }
        };
        assert!(accepted(true, None, |_| panic!("no group, no answer")));
        assert!(!accepted(false, None, |_| panic!("no group, no answer")));
        assert!(!accepted(false, Some(&male), utility(5.0)));
        assert_eq!(asked.get(), 0, "a taken man draws nothing");
        assert!(accepted(true, Some(&male), utility(0.5)));
        assert_eq!(asked.get(), 1);
        assert!(!accepted(true, Some(&male), utility(0.0)));
        assert!(!accepted(true, Some(&male), utility(-0.5)));
        assert_eq!(asked.get(), 3);
    }

    /// Two new addresses (11.7): his Gumbel is addressed by his key, the month and her key;
    /// the taste by the person alone.
    #[test]
    fn the_new_draws_are_addressed_as_the_specification_says() {
        let (her, his) = (
            LineageKey::founder(&data_id("test:a"), 0),
            LineageKey::founder(&data_id("test:a"), 1),
        );
        assert_eq!(
            accept_key(his, 7, her),
            DrawKey {
                tag: tags::PAIRING_ACCEPT,
                subject: SubjectKey::from(his),
                occasion: 7,
                detail: her.get(),
            }
        );
        assert_ne!(accept_key(his, 7, her), noise_key(her, 7, his));
        assert_eq!(
            taste_key(her),
            DrawKey {
                tag: tags::PAIRING_TASTE,
                subject: SubjectKey::from(her),
                occasion: 0,
                detail: 0,
            }
        );
    }

    fn data_id(text: &str) -> crate::data_id::DataId {
        crate::data_id::DataId::new(text).unwrap_or_else(|_| panic!("a data id"))
    }

    /// The taste is `normal(0, σ_ν)` at the key of the person with the seed of its lot: a pure
    /// function. Values of the independent draws of `tools/rng_reference.py` for the
    /// founders 1 and 0 of `test:answer` (seed 0xff, σ_ν = 3): 5.400892368330496 and
    /// −5.1569021306108365. The scale is exact (`0 + σ · z`), another seed or person draws
    /// another number.
    #[test]
    fn the_taste_is_a_pure_normal_draw_of_the_lots_seed() {
        let id = data_id("test:answer");
        let (hers, his) = (LineageKey::founder(&id, 0), LineageKey::founder(&id, 1));
        assert!((taste(0xff, his, 3.0) - 5.400_892_368_330_496).abs() <= 1e-12);
        assert!((taste(0xff, hers, 3.0) - -5.156_902_130_610_836_5).abs() <= 1e-12);
        assert_eq!(taste(0xff, his, 3.0), taste(0xff, his, 3.0));
        assert_eq!(taste(0xff, his, 3.0), 3.0 * taste(0xff, his, 1.0));
        assert_eq!(taste(0xff, his, 0.0), 0.0);
        assert_ne!(taste(0xfe, his, 3.0), taste(0xff, his, 3.0));
        assert_ne!(taste(0xff, hers, 3.0), taste(0xff, his, 3.0));
    }

    /// A person of the town draws its taste with the world seed (ADR-0020 section 5, as the
    /// wish does). A person of the pool draws with the pool's seed: that is the other branch
    /// of `Lot::wish_of`, the seed half of which `Lot::seed` returns unchanged.
    #[test]
    fn a_town_born_person_draws_with_the_world_seed() {
        let data = crate::data::testing::data();
        assert_eq!(crate::state::conception::Lot::Town(0xff).seed(&data), 0xff);
        assert_eq!(crate::state::conception::Lot::Town(7).seed(&data), 7);
    }
}
