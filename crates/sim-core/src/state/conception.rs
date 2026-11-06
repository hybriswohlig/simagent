//! Conception (genetics specification 5.6 and 11.3, ADR-0017 section 4 and its addendum of
//! 2026-11-05, B18): the woman of an open partnership tries once a month while the couple's
//! gate is open (`decide`), she is not pregnant, past the pause after a birth of this
//! partnership and her fertility has not ended. The chance is the adult rate times her
//! factor from her menarche and his from his puberty timing, both at the moment of the
//! attempt. A successful draw fixes the child at once: its lineage key from both parents
//! and her conception number, one meiosis per parent and the sex chromosome of the father.
//! The pregnancy is state; menarche and both wishes are pure functions of the seed.

use alloc::vec;
use alloc::vec::Vec;

use super::{DomainState, Pregnancy, Slot, Unborn, disease, partnership, together};
use crate::data::Data;
use crate::data_id::Code;
use crate::data_id::DataId;
use crate::demography::ExtraChild;
use crate::fertility::{self, Curves};
use crate::genome::Genome;
use crate::genome::meiosis::Zygote;
use crate::ids::{EventId, PartnershipId, SimId};
use crate::lineage::{GenomeKey, LineageKey};
use crate::log::{Cause, Causes, DrawRef, LogPayload, Subject};
use crate::phenotype::{self, Parents};
use crate::pool::PoolPerson;
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{MONTH, SimDuration, SimTime, YEAR};
use crate::world::{EventPayload, SimCtx};

/// The first month start after `t`; `None` beyond the clock.
pub(crate) fn month_start_after(t: SimTime) -> Option<SimTime> {
    let index = t.as_secs().div_euclid(MONTH.as_secs()).checked_add(1)?;
    SimTime::try_from(index.checked_mul(MONTH.as_secs())?).ok()
}

/// The first month start at or after `t`; `None` beyond the clock.
pub(crate) fn month_start_at_or_after(t: SimTime) -> Option<SimTime> {
    let month = MONTH.as_secs();
    let index = t.as_secs().div_euclid(month) + i64::from(t.as_secs().rem_euclid(month) != 0);
    SimTime::try_from(index.checked_mul(month)?).ok()
}

/// When the series of a woman begins after her partnership formed at `formed`: the first
/// month start after it (ADR-0017 section 4).
pub(crate) fn series_after_pairing(formed: SimTime) -> Option<SimTime> {
    month_start_after(formed)
}

/// When the series of a woman begins after a birth at `born`: the first month start at or
/// after the pause, never in the second of the birth (ADR-0017 section 4).
pub(crate) fn series_after_birth(born: SimTime, pause_months: u32) -> Option<SimTime> {
    let pause = MONTH.checked_mul(i64::from(pause_months))?;
    let one = SimDuration::from_secs(1);
    let wait = if pause > one { pause } else { one };
    month_start_at_or_after(born.checked_add(wait)?)
}

/// The age from which her factor stays 0: the end of the decline, or of the window.
pub(crate) fn fertility_end(curves: &Curves) -> f64 {
    match curves {
        Curves::Puberty(p) => p.female_decline_to,
        Curves::Window { until, .. } => *until,
    }
}

/// What the rule reads of a person: born, lineage key, genome and the lineage keys of
/// the biological parents in the run.
pub(crate) struct Person<'a> {
    pub(crate) born: SimTime,
    pub(crate) lineage_key: LineageKey,
    pub(crate) genome: &'a Genome,
    pub(crate) parents: Option<Parents>,
    /// The seeds of its draws (design note of the arrival, section 6.2).
    pub(crate) seeds: super::Seeds,
}

impl Person<'_> {
    /// Its phenotype of `t` at the age `age`, `E` and `C` from its seeds.
    pub(crate) fn phenotype(&self, data: &Data, t: &DataId, age: SimDuration) -> Option<f64> {
        let individual = Draws::new(WorldSeed::new(self.seeds.individual));
        let shared = Draws::new(WorldSeed::new(self.seeds.shared));
        phenotype::phenotype(
            phenotype::Environment {
                individual: &individual,
                shared: &shared,
                neutral: false,
            },
            data,
            self.genome,
            self.lineage_key,
            self.parents,
            t,
            age,
        )
    }
}

/// The puberty timing `θ_i` of a person at `t` (5.2): from the phenotype of the trait the
/// data name, else `θ` of the curve of the sex (11.1: the term drops).
fn theta_i(data: &Data, person: &Person<'_>, t: SimTime) -> f64 {
    let genetics = data.genetics();
    let curve = genetics.growth(person.genome.sex());
    let z_p = data
        .puberty_trait()
        .and_then(|t_p| person.phenotype(data, t_p, t.checked_since(person.born)?))
        .unwrap_or(0.0);
    fertility::theta_i(curve, z_p)
}

/// Whose wish and draws a person has (ADR-0017, addendum of 2026-11-05): a pool person
/// keeps the extract's (the pool seed, the shape and tilt the pool was checked against),
/// anyone else draws with the world seed and the data's shape and tilt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lot {
    Town(u64),
    Pool,
}

impl Lot {
    /// The lot of a living or archived sim, by its origin.
    pub(crate) fn of(domain: &DomainState, sim: SimId) -> Lot {
        if domain.pool_origin(sim).is_some() {
            Lot::Pool
        } else {
            Lot::Town(
                domain
                    .world_seed
                    .unwrap_or_else(|| panic!("invariant seeds.before_start")),
            )
        }
    }

    /// The seed of its draws: the world seed of a town person, the pool's of a pool person
    /// (the seed half of [`Lot::wish_of`]; the taste of the partner market draws with it, 11.2a).
    pub(crate) fn seed(self, data: &Data) -> u64 {
        self.wish_of(data).0
    }

    /// The seed of its draws and the wish shape and tilt it reads.
    fn wish_of(self, data: &Data) -> (u64, &[f64], f64) {
        match self {
            Lot::Town(seed) => {
                let fertility = &data.demography().fertility;
                (seed, &fertility.wish_shape, fertility.wish_tilt)
            }
            Lot::Pool => {
                let pool = data
                    .pool()
                    .unwrap_or_else(|| panic!("invariant pool.missing"));
                let (shape, tilt) = pool.wish();
                (pool.pool().pool_seed, shape, tilt)
            }
        }
    }
}

/// The address of a person's draw of `FERTILITY_WISH` at `occasion`: 0 the wish, 1 whether
/// a grown child may reopen her.
fn wish_key(person: LineageKey, occasion: u64) -> DrawKey {
    DrawKey {
        tag: tags::FERTILITY_WISH,
        subject: SubjectKey::from(person),
        occasion,
        detail: 0,
    }
}

/// The wish for children of a woman or a man (11.3; his since ADR-0017, addendum of
/// 2026-11-05): a pure function of the lot and the lineage key.
pub(crate) fn wish(data: &Data, lot: Lot, person: LineageKey) -> u32 {
    let (seed, shape, tilt) = lot.wish_of(data);
    let u = Draws::new(WorldSeed::new(seed)).unit(wish_key(person, 0));
    fertility::wish(u, shape, tilt)
}

/// Whether a grown child may reopen her (ADR-0017, addendum of 2026-11-05): her draw at
/// occasion 1, with the seed of her wish, below the grown chance.
pub(crate) fn may_reopen(data: &Data, lot: Lot, her: LineageKey, chance: f64) -> bool {
    let (seed, _, _) = lot.wish_of(data);
    Draws::new(WorldSeed::new(seed)).unit(wish_key(her, 1)) < chance
}

/// What the gate reads of a couple at a month start `t` (ADR-0017, addendum of 2026-11-05).
/// Two places are meant for later terms of circumstance (housing against means, a partner's
/// wealth, a per-person sensitivity drawn like the wish, education): the effective wish
/// ceiling, here `her_wish` and `his_wish` (today the drawn wishes), and the monthly chance
/// of an attempt (`chance`, today the rate times both factors).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Couple {
    pub(crate) her_wish: u32,
    pub(crate) his_wish: u32,
    /// Her live births, of any partner, the pool's included.
    pub(crate) her_life: u32,
    /// Her live births less her children with a town birthday by `t` and 18 whole years.
    pub(crate) her_young: u32,
    /// His children in the run under 18 whole years, and the pool's persons naming him who
    /// have not come.
    pub(crate) his_young: u32,
    /// The live births of this pair.
    pub(crate) ours: u32,
    /// Her draw lets a grown child reopen her.
    pub(crate) may_reopen: bool,
    pub(crate) pregnant: bool,
    /// The pause after a birth of this partnership still runs.
    pub(crate) paused: bool,
    pub(crate) her_age: f64,
    pub(crate) fertility_end: f64,
    pub(crate) rule: Option<ExtraChild>,
}

/// Why a couple tries: her wish, his wish in a new partnership, a grown child of hers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Opening {
    Hers,
    His,
    Grown,
}

/// Why a couple does not try now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Held {
    FertilityOver,
    Pregnant,
    Pause,
    Fulfilled,
}

/// The decision at a month start (ADR-0017, addendum of 2026-11-05): her fertility over,
/// pregnant, the pause, then the gate. She is open when `young < W`, `life < W + extra` and
/// (`life < W`, or `life >= grown_from` and her draw lets a grown child reopen her); otherwise
/// his slot when `W >= 1`, `W <= life < W + extra`, his young below his wish and no live birth
/// of the pair yet. Without the extra child (none in the data, or she has a pool origin,
/// `PairFacts::rule`) she is open while `life < W`.
pub(crate) fn decide(c: &Couple) -> Result<Opening, Held> {
    if c.her_age >= c.fertility_end {
        return Err(Held::FertilityOver);
    }
    if c.pregnant {
        return Err(Held::Pregnant);
    }
    if c.paused {
        return Err(Held::Pause);
    }
    let w = c.her_wish;
    let Some(rule) = c.rule else {
        return if c.her_life < w {
            Ok(Opening::Hers)
        } else {
            Err(Held::Fulfilled)
        };
    };
    let below_extra = u64::from(c.her_life) < u64::from(w) + u64::from(rule.extra);
    if !below_extra {
        return Err(Held::Fulfilled);
    }
    if c.her_life < w {
        // `young <= life < W`.
        return Ok(Opening::Hers);
    }
    if c.her_young < w && c.her_life >= rule.grown_from && c.may_reopen {
        return Ok(Opening::Grown);
    }
    if w >= 1 && c.his_young < c.his_wish && c.ours < 1 {
        return Ok(Opening::His);
    }
    Err(Held::Fulfilled)
}

/// A child as the gate counts it: its birth second, for a pool member that came its
/// arrival, and its parents' lineage keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Child {
    pub(crate) born: SimTime,
    pub(crate) arrived: Option<SimTime>,
    pub(crate) mother: Option<LineageKey>,
    pub(crate) father: Option<LineageKey>,
}

impl Child {
    /// Whether it has a town birthday at `t`: born in the town by `t`, or arrived before
    /// `t` (an arrival counts from the first month start after its second).
    fn seen(&self, t: SimTime) -> bool {
        match self.arrived {
            None => self.born <= t,
            Some(arrived) => arrived < t,
        }
    }

    /// Whether `t` lies at or after the first month start of its `years` whole years.
    fn grown(&self, t: SimTime, years: u32) -> bool {
        YEAR.checked_mul(i64::from(years))
            .and_then(|span| self.born.checked_add(span))
            .is_some_and(|at| t >= at)
    }
}

/// Her live births less her children with a town birthday by `t` and grown.
pub(crate) fn her_young(life: u32, hers: &[Child], t: SimTime, years: u32) -> u32 {
    let grown = hers
        .iter()
        .filter(|child| child.seen(t) && child.grown(t, years))
        .count();
    life.saturating_sub(u32::try_from(grown).unwrap_or(u32::MAX))
}

/// His children in the run not grown at `t` (a pool member that had not come yet counts),
/// and `unseen`, the pool's persons naming him who never came.
pub(crate) fn his_young(his: &[Child], unseen: u32, t: SimTime, years: u32) -> u32 {
    let young = his
        .iter()
        .filter(|child| match child.arrived {
            None => child.born <= t && !child.grown(t, years),
            Some(_) => !child.seen(t) || !child.grown(t, years),
        })
        .count();
    u32::try_from(young)
        .unwrap_or(u32::MAX)
        .saturating_add(unseen)
}

/// The live births of the pair by `t` among her children born in the town, and `members`,
/// the pool's members naming both.
pub(crate) fn ours(hers: &[Child], him: LineageKey, members: u32, t: SimTime) -> u32 {
    let town = hers
        .iter()
        .filter(|child| child.arrived.is_none() && child.father == Some(him) && child.born <= t)
        .count();
    u32::try_from(town)
        .unwrap_or(u32::MAX)
        .saturating_add(members)
}

/// The month starts from `start` on at which the gate can open without a birth: `start`,
/// each at which a child of either turns `years`, each first month start after an arrival.
pub(crate) fn candidates(
    start: SimTime,
    children: &[&[Child]],
    rule: Option<ExtraChild>,
) -> Vec<SimTime> {
    let mut out = vec![start];
    if let Some(rule) = rule {
        let span = YEAR.checked_mul(i64::from(rule.count_until_years));
        for child in children.iter().flat_map(|list| list.iter()) {
            let grown = span.and_then(|span| child.born.checked_add(span));
            out.extend(grown.and_then(month_start_at_or_after));
            out.extend(child.arrived.and_then(month_start_after));
        }
    }
    out.retain(|t| *t >= start);
    out.sort_unstable();
    out.dedup();
    out
}

/// The first of `candidates` at which `couple_at` tries; none once her fertility has
/// ended. Between births the gate only opens, so this is the first month start of the
/// series.
pub(crate) fn first_open(
    candidates: &[SimTime],
    couple_at: impl Fn(SimTime) -> Couple,
) -> Option<SimTime> {
    for &t in candidates {
        let couple = couple_at(t);
        match decide(&couple) {
            Ok(_) => return Some(t),
            Err(Held::FertilityOver) => return None,
            Err(_) => {}
        }
    }
    None
}

/// The chance of her attempt with him at `t` (11.3): `rate` (the adult rate, or the boost of
/// "Zeit zu zweit") times her factor from her menarche and his from his puberty timing,
/// both at `t`.
pub(crate) fn chance(
    data: &Data,
    her: &Person<'_>,
    him: &Person<'_>,
    t: SimTime,
    rate: f64,
) -> f64 {
    let curves = data.genetics().fertility;
    let her_menarche = match &curves {
        Curves::Puberty(p) => {
            let key = DrawKey {
                tag: tags::FERTILITY_MENARCHE,
                subject: SubjectKey::from(her.lineage_key),
                occasion: 0,
                detail: 0,
            };
            let delta = Draws::new(WorldSeed::new(her.seeds.life)).normal(
                key,
                p.menarche_mean,
                p.menarche_sd,
            );
            fertility::menarche(theta_i(data, her, t), delta, p)
        }
        Curves::Window { .. } => 0.0,
    };
    let female = fertility::female_factor(disease::age(her.born, t), her_menarche, &curves);
    let male = fertility::male_factor(disease::age(him.born, t), theta_i(data, him, t), &curves);
    fertility::chance(rate, female, male)
}

/// The rate of an attempt of `partnership` at `t` with `window` (ADR-0023, addendum of
/// 2026-11-01): the boost of the template inside the window, else the adult rate.
pub(crate) fn rate(
    data: &Data,
    window: Option<&together::Window>,
    partnership: PartnershipId,
    t: SimTime,
) -> f64 {
    let adult = data.demography().fertility.adult_rate;
    match together::template(data) {
        Some((_, boost)) => together::rate_at(window, partnership, t, adult, boost.get()),
        None => adult,
    }
}

/// What the gate reads of a couple, gathered once (by the core from the domain, by the
/// loader from a save), at any month start `t` through `couple`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PairFacts {
    pub(crate) him: LineageKey,
    pub(crate) her_born: SimTime,
    pub(crate) her_wish: u32,
    pub(crate) his_wish: u32,
    /// Her children in the run, and his.
    pub(crate) hers: Vec<Child>,
    pub(crate) his: Vec<Child>,
    /// The pool's persons naming him who never came.
    pub(crate) his_unseen: u32,
    /// The pool's members naming both.
    pub(crate) pool_ours: u32,
    /// She brought this partnership with only it and live births: they are theirs.
    pub(crate) brought_births: bool,
    pub(crate) may_reopen: bool,
    /// She has a pool origin (a person of the pool, a child of a pool pregnancy; the cut of
    /// her wish, `Lot::Pool`).
    pub(crate) her_pool: bool,
}

impl PairFacts {
    /// The extra child her gate reads (ADR-0017, addendum of 2026-11-05, Grok's round 5):
    /// none for a woman with a pool origin, who keeps the extract's wish and its gate (open
    /// while her live births are below her wish), the data's for everyone else.
    fn rule(&self, data: &Data) -> Option<ExtraChild> {
        if self.her_pool {
            None
        } else {
            data.demography().fertility.extra
        }
    }

    /// The couple at `t` with her live births `life` by then.
    pub(crate) fn couple(
        &self,
        data: &Data,
        t: SimTime,
        life: u32,
        (pregnant, paused): (bool, bool),
    ) -> Couple {
        let rule = self.rule(data);
        let years = rule.map_or(0, |rule| rule.count_until_years);
        let ours = ours(&self.hers, self.him, self.pool_ours, t);
        Couple {
            her_wish: self.her_wish,
            his_wish: self.his_wish,
            her_life: life,
            her_young: her_young(life, &self.hers, t, years),
            his_young: his_young(&self.his, self.his_unseen, t, years),
            ours: if self.brought_births {
                ours.max(1)
            } else {
                ours
            },
            may_reopen: self.may_reopen,
            pregnant,
            paused,
            her_age: disease::age(self.her_born, t),
            fertility_end: fertility_end(&data.genetics().fertility),
            rule,
        }
    }

    /// The month starts from `start` on at which the gate can open (`candidates`).
    pub(crate) fn candidates(&self, data: &Data, start: SimTime) -> Vec<SimTime> {
        candidates(start, &[&self.hers, &self.his], self.rule(data))
    }
}

/// A child of the domain as the gate counts it.
fn child(domain: &DomainState, id: SimId) -> Child {
    let born = domain
        .sims
        .get(&id)
        .map(|sim| sim.born)
        .or_else(|| domain.archive.get(&id).map(|sim| sim.born))
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", id.get()));
    if let Some(origin) = domain.pool_origin(id) {
        return Child {
            born,
            arrived: origin.born_pool.map(|_| origin.arrived),
            mother: origin.mother,
            father: origin.father,
        };
    }
    let parents = domain.parent_keys(id);
    Child {
        born,
        arrived: None,
        mother: parents.map(|p| p.mother),
        father: parents.map(|p| p.father),
    }
}

/// The children in the run of the person with this lineage key.
pub(crate) fn children_of(domain: &DomainState, key: LineageKey) -> Vec<Child> {
    domain
        .offspring
        .get(&key)
        .map(|list| list.iter().map(|id| child(domain, *id)).collect())
        .unwrap_or_default()
}

/// Notes `child` under the lineage keys of its parents (`DomainState::offspring`).
pub(crate) fn note_child(domain: &mut DomainState, child: SimId) {
    let fact = self::child(domain, child);
    for key in [fact.mother, fact.father].into_iter().flatten() {
        domain.offspring.entry(key).or_default().push(child);
    }
}

/// Her pool record and the shift of her household, if `partnership` is the one her
/// household brought from the pool (ADR-0020 section 5): both partners came in the same
/// second as members of that household, and it formed at the pool's second shifted.
pub(crate) fn brought<'a>(
    domain: &DomainState,
    data: &'a Data,
    partnership: PartnershipId,
) -> Option<(&'a PoolPerson, i64)> {
    let pair = domain.partnerships.get(&partnership)?;
    let (her, his) = (
        domain.pool_origin(pair.woman)?,
        domain.pool_origin(pair.man)?,
    );
    if her.born_pool.is_none() || his.born_pool.is_none() || her.arrived != his.arrived {
        return None;
    }
    let pool = data.pool()?;
    let (household, record) = pool.member(domain.key_of(pair.woman))?;
    let brought = household.partnership?;
    let delta = her
        .arrived
        .as_secs()
        .checked_sub(pool.pool().frozen_at.as_secs())?;
    let formed = brought.formed.as_secs().checked_add(delta)?;
    (brought.woman == domain.key_of(pair.woman)
        && brought.man == domain.key_of(pair.man)
        && formed == pair.formed.as_secs())
    .then_some((record, delta))
}

/// What the gate reads of the couple of her open partnership `id` with him.
pub(crate) fn pair_facts(
    domain: &DomainState,
    data: &Data,
    (woman, man, id): (SimId, SimId, PartnershipId),
) -> PairFacts {
    let (her, him) = (domain.key_of(woman), domain.key_of(man));
    let (her_lot, his_lot) = (Lot::of(domain, woman), Lot::of(domain, man));
    let born = domain
        .sims
        .get(&woman)
        .map(|sim| sim.born)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", woman.get()));
    let fathered = data.pool().map_or(&[][..], |pool| pool.fathered(him));
    let his_unseen = fathered
        .iter()
        .filter(|(child, _)| !domain.keys.contains_key(child))
        .count();
    let pool_ours = fathered
        .iter()
        .filter(|(_, mother)| *mother == Some(her))
        .count();
    let brought_births = brought(domain, data, id)
        .is_some_and(|(record, _)| record.partnerships == 1 && record.births >= 1);
    let chance = data
        .demography()
        .fertility
        .extra
        .map_or(0.0, |rule| rule.grown_chance);
    PairFacts {
        him,
        her_born: born,
        her_wish: wish(data, her_lot, her),
        his_wish: wish(data, his_lot, him),
        hers: children_of(domain, her),
        his: children_of(domain, him),
        his_unseen: u32::try_from(his_unseen).unwrap_or(u32::MAX),
        pool_ours: u32::try_from(pool_ours).unwrap_or(u32::MAX),
        brought_births,
        may_reopen: may_reopen(data, her_lot, her, chance),
        her_pool: her_lot == Lot::Pool,
    }
}

/// Whether her open partnership `id` still runs the series she brought: it is the one her
/// household brought and no child of hers was born in the town (design note of the carried
/// plans); `Some(true)` if the pool carried a series, `Some(false)` if not.
pub(crate) fn carried(
    domain: &DomainState,
    data: &Data,
    woman: SimId,
    id: PartnershipId,
) -> Option<bool> {
    let (record, _) = brought(domain, data, id)?;
    let town_child = children_of(domain, domain.key_of(woman))
        .iter()
        .any(|child| child.arrived.is_none());
    (!town_child).then_some(record.conception_at.is_some())
}

/// Where the town's rule may begin a series in a partnership she brought without one: the
/// first month start after her arrival, or the end of the pause after her last birth in it
/// before the freeze (pool seconds, shifted) if later (ADR-0017, addendum of 2026-11-05).
pub(crate) fn brought_start(
    data: &Data,
    hers: &[(SimTime, Option<SimTime>)],
    arrived: SimTime,
    pool: (SimTime, i64),
) -> Option<SimTime> {
    let first = month_start_after(arrived)?;
    Some(match brought_pause(data, hers, pool)? {
        Some(after) => first.max(after),
        None => first,
    })
}

/// The end of the pause after her last birth before the freeze in the partnership she
/// brought (formed at `formed_pool` in the pool, her household shifted by `delta`): the first
/// month start at or after it; `Some(None)` without such a birth.
fn brought_pause(
    data: &Data,
    hers: &[(SimTime, Option<SimTime>)],
    (formed_pool, delta): (SimTime, i64),
) -> Option<Option<SimTime>> {
    let last = hers
        .iter()
        .filter_map(|(_, born_pool)| *born_pool)
        .filter(|born| *born > formed_pool)
        .max();
    let Some(last) = last else {
        return Some(None);
    };
    let pause = data.demography().fertility.pause_months;
    series_after_birth(
        SimTime::from_secs(last.as_secs().checked_add(delta)?),
        pause,
    )
    .map(Some)
}

/// The pool's side of the partnership `id` she brought: her children in the run by their
/// arrival and their birth second in the pool, her arrival, and the partnership's forming in
/// the pool with the shift of her household (what `brought_start` reads).
#[allow(
    clippy::type_complexity,
    reason = "the three parts `brought_start` takes, named there"
)]
fn brought_side(
    domain: &DomainState,
    data: &Data,
    woman: SimId,
    id: PartnershipId,
) -> Option<(Vec<(SimTime, Option<SimTime>)>, SimTime, (SimTime, i64))> {
    let key = domain.key_of(woman);
    let (household, _) = data.pool()?.member(key)?;
    let formed_pool = household.partnership?.formed;
    let (_, delta) = brought(domain, data, id)?;
    let origin = domain.pool_origin(woman)?;
    let hers: Vec<(SimTime, Option<SimTime>)> = (domain.offspring.get(&key).into_iter())
        .flatten()
        .filter_map(|child| domain.pool_origin(*child))
        .map(|origin| (origin.arrived, origin.born_pool))
        .collect();
    Some((hers, origin.arrived, (formed_pool, delta)))
}

/// The end of the pause after her last birth in her open partnership `id` (ADR-0017
/// section 4 and its addendum of 2026-11-05): after her last birth in the town in it, else,
/// for a partnership she brought, after her last birth in it before the freeze (pool
/// seconds, shifted, as `brought_start` reads it); `None` without one.
pub(crate) fn pause_end(
    domain: &DomainState,
    data: &Data,
    woman: SimId,
    id: PartnershipId,
) -> Option<SimTime> {
    if let Some(born) = last_town_birth(domain, woman, id) {
        return series_after_birth(born, data.demography().fertility.pause_months);
    }
    let (hers, _, pool) = brought_side(domain, data, woman, id)?;
    brought_pause(data, &hers, pool)?
}

/// Where the town's rule begins her series in her open partnership `id`: after its forming,
/// after the pause of her last birth in the town in it (a birth in the second of a pairing
/// by an input comes after it), or for a partnership she brought without a series
/// `brought_start`.
pub(crate) fn series_start(
    domain: &DomainState,
    data: &Data,
    woman: SimId,
    id: PartnershipId,
) -> Option<SimTime> {
    let pair = domain.partnerships.get(&id)?;
    if carried(domain, data, woman, id) == Some(false) {
        let (hers, arrived, pool) = brought_side(domain, data, woman, id)?;
        return brought_start(data, &hers, arrived, pool);
    }
    match last_town_birth(domain, woman, id) {
        Some(born) => series_after_birth(born, data.demography().fertility.pause_months),
        None => series_after_pairing(pair.formed),
    }
}

/// Her last birth in the town in her partnership `id`: after its forming, or in its second
/// if an input formed it (the input acts before the events of its second).
pub(crate) fn last_town_birth(
    domain: &DomainState,
    woman: SimId,
    id: PartnershipId,
) -> Option<SimTime> {
    let pair = domain.partnerships.get(&id)?;
    let arranged = domain.arranged.contains(&id);
    children_of(domain, domain.key_of(woman))
        .iter()
        .filter(|child| child.arrived.is_none())
        .map(|child| child.born)
        .filter(|born| *born > pair.formed || (*born == pair.formed && arranged))
        .max()
}

/// The first month start from `start` on at which she tries in her open partnership, given
/// no birth before it; none if she is pregnant or has none.
pub(crate) fn next_open(
    domain: &DomainState,
    data: &Data,
    woman: SimId,
    start: SimTime,
) -> Option<SimTime> {
    let id = partnership::open_partnership(domain, woman)?;
    if domain.pregnancies.contains_key(&woman) {
        return None;
    }
    let man = domain.partnerships.get(&id)?.man;
    let facts = pair_facts(domain, data, (woman, man, id));
    let life = domain.sims.get(&woman)?.births;
    first_open(&facts.candidates(data, start), |t| {
        facts.couple(data, t, life, (false, false))
    })
}

/// The facts of a sim of the domain the rule reads.
pub(crate) fn person<'a>(domain: &'a DomainState, data: &Data, id: SimId) -> Person<'a> {
    let Some(sim) = domain.sims.get(&id) else {
        panic!("invariant unknown_entity_id: sim {}", id.get());
    };
    Person {
        born: sim.born,
        lineage_key: sim.lineage_key,
        genome: &sim.genome,
        parents: domain.parent_keys(id),
        seeds: domain.seeds(data, id),
    }
}

/// Plans the monthly attempts of `woman` from the first month start at or after `start`
/// (after her pairing, after a birth, or the town's start of a partnership she brought) at
/// which she tries (`next_open`), by `by`; nothing if there is none.
pub(crate) fn plan_attempts(
    domain: &mut DomainState,
    data: &Data,
    woman: SimId,
    by: EventId,
    start: Option<SimTime>,
    ctx: &mut SimCtx<'_, '_>,
) {
    if !domain.sims.contains_key(&woman) {
        panic!("invariant unknown_entity_id: sim {}", woman.get());
    }
    let Some(at) = start.and_then(|start| next_open(domain, data, woman, start)) else {
        return;
    };
    let payload = EventPayload::ConceptionAttempt { woman };
    let handle = ctx.schedule_periodic(at, MONTH, payload, by);
    domain.pending.insert((woman, Slot::Conception), handle);
}

/// After a fact her gate reads changed beside her own births (a child of her partner born
/// of another woman, an arrival naming her or him; ADR-0017, addendum of 2026-11-05): her
/// pending series is the first month start from now on at which she tries. It stays if it
/// is, else it is cancelled and planned again by `by`. A series she brought stays.
pub(crate) fn replan(
    domain: &mut DomainState,
    data: &Data,
    woman: SimId,
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    if !domain.sims.contains_key(&woman) || domain.pregnancies.contains_key(&woman) {
        return;
    }
    let Some(id) = partnership::open_partnership(domain, woman) else {
        return;
    };
    if carried(domain, data, woman, id) == Some(true) {
        return;
    }
    let now = month_start_at_or_after(ctx.now());
    let from = series_start(domain, data, woman, id)
        .zip(now)
        .map(|(a, b)| a.max(b));
    let want = from.and_then(|from| next_open(domain, data, woman, from));
    let have = (domain.pending.get(&(woman, Slot::Conception))).and_then(|h| ctx.pending_time(*h));
    if have == want {
        return;
    }
    stop_attempts(domain, woman, ctx);
    if let Some(at) = want {
        let payload = EventPayload::ConceptionAttempt { woman };
        let handle = ctx.schedule_periodic(at, MONTH, payload, by);
        domain.pending.insert((woman, Slot::Conception), handle);
    }
}

/// Ends the attempts of `woman`, if she has any.
pub(crate) fn stop_attempts(domain: &mut DomainState, woman: SimId, ctx: &mut SimCtx<'_, '_>) {
    if let Some(handle) = domain.pending.remove(&(woman, Slot::Conception)) {
        ctx.cancel(handle);
    }
}

/// The address of the attempt of a woman with a man in a month.
pub(crate) fn attempt_key(her: LineageKey, him: LineageKey, month: u64) -> DrawKey {
    DrawKey {
        tag: tags::CONCEPTION_ATTEMPT,
        subject: SubjectKey::from(her),
        occasion: month,
        detail: him.get(),
    }
}

/// The month index of a second at or after the start.
pub(crate) fn month_index(t: SimTime) -> Option<u64> {
    u64::try_from(t.as_secs().div_euclid(MONTH.as_secs())).ok()
}

/// An attempt of the monthly series of `woman`, at the rate of her `window` of "Zeit zu
/// zweit" if it holds: the draw decides. A failed one writes no entry; it ends the series if
/// her fertility has ended by the next month start, so that no attempt lies at or after its
/// end.
pub(crate) fn on_attempt(
    domain: &mut DomainState,
    data: &Data,
    woman: SimId,
    window: Option<&together::Window>,
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant conception.before_start"));
    let now = ctx.now();
    let Some((id, man)) = partnership::open_partnership(domain, woman).and_then(|id| {
        domain
            .partnerships
            .get(&id)
            .map(|partnership| (id, partnership.man))
    }) else {
        panic!("invariant conception.without_partnership");
    };
    let (her, him) = (person(domain, data, woman), person(domain, data, man));
    let month = month_index(now).unwrap_or_else(|| panic!("invariant conception.before_start"));
    let draw = attempt_key(her.lineage_key, him.lineage_key, month);
    let p = chance(data, &her, &him, now, rate(data, window, id, now));
    if !Draws::new(WorldSeed::new(seed)).bernoulli(draw, p) {
        let end = fertility_end(&data.genetics().fertility);
        let next = now.checked_add(MONTH);
        if next.is_none_or(|next| disease::age(her.born, next) >= end) {
            stop_attempts(domain, woman, ctx);
        }
        return;
    }
    stop_attempts(domain, woman, ctx);
    conceive(domain, data, seed, woman, man, draw, ctx);
}

/// How many children a conception gives (spec 11.3): one, identical or fraternal twins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Multiple {
    One,
    Identical,
    Fraternal,
}

/// The address of the draw that decides how many children a conception gives: by the key
/// of its first child (11.7).
pub(crate) fn multiple_key(first: LineageKey) -> DrawKey {
    DrawKey {
        tag: tags::MULTIPLE_BIRTH,
        subject: SubjectKey::from(first),
        occasion: 0,
        detail: 0,
    }
}

/// One child, identical or fraternal twins (11.3): identical below the rate of identical
/// twins, fraternal below the sum of both rates.
pub(crate) fn multiple(data: &Data, seed: u64, first: LineageKey) -> Multiple {
    let u = Draws::new(WorldSeed::new(seed)).unit(multiple_key(first));
    let twins = data.demography().twins;
    if u < twins.identical {
        Multiple::Identical
    } else if u < twins.identical + twins.fraternal {
        Multiple::Fraternal
    } else {
        Multiple::One
    }
}

/// The children of her conception number `number` with him (ADR-0001 section 4, 11.3):
/// per child its lineage key, the genome key of its zygote and the zygote; identical twins
/// share the zygote of the first child.
pub(crate) fn children(
    data: &Data,
    seed: u64,
    (her_key, her_genome): (LineageKey, &Genome),
    (his_key, his_genome): (LineageKey, &Genome),
    number: u32,
) -> alloc::vec::Vec<(LineageKey, GenomeKey, Zygote)> {
    let key = |index: u64| LineageKey::child(her_key, his_key, u64::from(number), index);
    let draws = Draws::new(WorldSeed::new(seed));
    let zygote = |genome: GenomeKey| Zygote::of(&draws, data, genome, her_genome, his_genome);
    let first = (key(0), GenomeKey::of_zygote(key(0)));
    let first = (first.0, first.1, zygote(first.1));
    match multiple(data, seed, first.0) {
        Multiple::One => vec![first],
        Multiple::Identical => {
            let twin = (key(1), first.1, first.2.clone());
            vec![first, twin]
        }
        Multiple::Fraternal => {
            let genome = GenomeKey::of_zygote(key(1));
            let twin = (key(1), genome, zygote(genome));
            vec![first, twin]
        }
    }
}

/// She conceives her next child or children with him now: per child in the order of the
/// index the entry of the conception, after it the two meioses of its zygote (genetics
/// specification 2.3; identical twins share those of the first), and the pregnancy. The
/// birth is planned by the entry of the first child.
fn conceive(
    domain: &mut DomainState,
    data: &Data,
    seed: u64,
    woman: SimId,
    man: SimId,
    draw: DrawKey,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some((his_key, his_genome)) = domain
        .sims
        .get(&man)
        .map(|him| (him.lineage_key, him.genome))
    else {
        panic!("invariant unknown_entity_id: sim {}", man.get());
    };
    let Some(her) = domain.sims.get_mut(&woman) else {
        panic!("invariant unknown_entity_id: sim {}", woman.get());
    };
    let number = her.conceptions;
    her.conceptions = number
        .checked_add(1)
        .unwrap_or_else(|| panic!("invariant conception.count_range"));
    let her_key = her.lineage_key;
    let her_genome = her.genome;
    let children = children(
        data,
        seed,
        (her_key, &her_genome),
        (his_key, &his_genome),
        number,
    );
    let causes = Causes::new(ctx.trigger())
        .with(Cause::Random(DrawRef::from(draw)))
        .with(Cause::Random(DrawRef::from(multiple_key(
            LineageKey::child(her_key, his_key, u64::from(number), 0),
        ))));
    // ADR-0004 section 7: the causes of a meiosis are the conception and the rule; its
    // draws follow from the genome key and the side (ADR-0001).
    let rule =
        Code::new("core:meiosis").unwrap_or_else(|_| panic!("invariant conception.rule_code"));
    let mut first_entry = None;
    let mut unborn = alloc::vec::Vec::with_capacity(children.len());
    let mut written: Option<GenomeKey> = None;
    for (child, genome, zygote) in &children {
        domain.lineage.insert(*child);
        let conception = ctx.emit(
            LogPayload::Conception {
                lineage_key: *child,
            },
            vec![Subject::Sim(woman), Subject::Sim(man)],
            causes.clone(),
        );
        first_entry.get_or_insert(conception);
        if written != Some(*genome) {
            for (parent, meiosis) in [(woman, &zygote.mother), (man, &zygote.father)] {
                ctx.emit(
                    LogPayload::Meiosis {
                        genome_key: *genome,
                        segments: meiosis.segments.clone(),
                        sex_chromosome: meiosis.sex_chromosome,
                        mutations: meiosis.mutations.clone(),
                    },
                    vec![Subject::Sim(parent)],
                    Causes::new(Cause::Event(conception)).with(Cause::Rule(rule.clone())),
                );
            }
            written = Some(*genome);
        }
        unborn.push(Unborn {
            lineage_key: *child,
            sex: zygote.genome.sex(),
            genome: zygote.genome,
        });
    }
    let now = ctx.now();
    domain.pregnancies.insert(
        woman,
        Pregnancy {
            father: Some(man),
            pool_father: None,
            conception: number,
            children: unborn,
            conceived: now,
        },
    );
    let first_entry = first_entry.unwrap_or_else(|| panic!("invariant conception.children"));
    let due = birth_of(data, now).unwrap_or_else(|| panic!("invariant time.out_of_range"));
    let handle = ctx.schedule(due, EventPayload::Birth { mother: woman }, first_entry);
    domain.pending.insert((woman, Slot::Birth), handle);
}

/// When a child conceived at `conceived` is born: a pregnancy later (11.3).
pub(crate) fn birth_of(data: &Data, conceived: SimTime) -> Option<SimTime> {
    let months = data.demography().fertility.pregnancy_months;
    conceived.checked_add(MONTH.checked_mul(i64::from(months))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: i64) -> SimTime {
        SimTime::from_secs(secs)
    }

    const M: i64 = 2_419_200;

    /// ADR-0017 section 4: a series begins at the first month start after the pairing,
    /// never in its second.
    #[test]
    fn the_series_begins_after_the_pairing() {
        assert_eq!(series_after_pairing(t(0)), Some(t(M)));
        assert_eq!(series_after_pairing(t(M - 1)), Some(t(M)));
        assert_eq!(series_after_pairing(t(M)), Some(t(2 * M)));
        assert_eq!(series_after_pairing(t(-1)), Some(t(0)));
    }

    /// ADR-0017 section 4: after a birth on a month start, a pause of 12 months begins
    /// the series exactly twelve months later; without a pause one month later, never in
    /// the second of the birth; a birth off the grid waits for the next month start.
    #[test]
    fn the_series_begins_after_the_pause() {
        assert_eq!(series_after_birth(t(5 * M), 12), Some(t(17 * M)));
        assert_eq!(series_after_birth(t(5 * M), 0), Some(t(6 * M)));
        assert_eq!(series_after_birth(t(5 * M), 1), Some(t(6 * M)));
        assert_eq!(series_after_birth(t(5 * M + 7), 12), Some(t(18 * M)));
        assert_eq!(series_after_birth(t(5 * M + 7), 0), Some(t(6 * M)));
    }

    /// The chance of an attempt on the repository's data (5.6, 11.3) for a young couple at
    /// the start, seed 0xff, founders 0 and 1 of `test:chance`, from `tools/world_reference.py`:
    /// her menarche at 12.8021 (`θ_i` 12.3 and the drawn offset 0.5021), his `θ_i` 14.02. The
    /// chance rises with her ramp and with his.
    #[test]
    fn the_chance_of_a_young_couple() {
        let data = crate::data::testing::data();
        let tag = crate::data_id::DataId::new("test:chance").expect("id");
        let (women, men) = (
            Genome::reference(crate::genome::Sex::Female),
            Genome::reference(crate::genome::Sex::Male),
        );
        let year = crate::time::YEAR.as_secs();
        let person = |index: u64, years_tenths: i64, genome| Person {
            born: t(-years_tenths * year / 10),
            lineage_key: LineageKey::founder(&tag, index),
            genome,
            parents: None,
            seeds: super::super::Seeds::one(0xff),
        };
        let chance_at = |her: i64, him: i64| {
            let adult = data.demography().fertility.adult_rate;
            chance(
                &data,
                &person(0, her, &women),
                &person(1, him, &men),
                t(0),
                adult,
            )
        };
        assert_eq!(chance_at(130, 150), 0.002_324_437_937_216_047_3);
        assert_eq!(chance_at(135, 150), 0.025_551_524_660_079_46);
        assert_eq!(chance_at(140, 150), 0.065_380_556_293_989_41);
        assert_eq!(chance_at(150, 135), 0.003_763_278_905_772_448_5);
        assert_eq!(chance_at(170, 140), 0.053_638_399_999_999_954);
        assert_eq!(chance_at(170, 170), 0.2);
    }

    /// The couple of the design's examples (ADR-0017, addendum of 2026-11-05): extra 1, grown
    /// from 3, nothing held.
    fn couple(her: (u32, u32, u32), his: (u32, u32), ours: u32, may_reopen: bool) -> Couple {
        Couple {
            her_wish: her.0,
            her_life: her.1,
            her_young: her.2,
            his_wish: his.0,
            his_young: his.1,
            ours,
            may_reopen,
            pregnant: false,
            paused: false,
            her_age: 30.0,
            fertility_end: 50.0,
            rule: Some(ExtraChild {
                count_until_years: 18,
                extra: 1,
                grown_from: 3,
                grown_chance: 0.2,
            }),
        }
    }

    /// The gate of the design, case by case (her wish, life, young; his wish, young; the
    /// pair's births; her draw).
    #[test]
    fn the_gate_of_both_wishes() {
        // Below her wish she tries, whatever else.
        assert_eq!(
            decide(&couple((3, 2, 2), (0, 9), 2, false)),
            Ok(Opening::Hers)
        );
        // The widow of the design: wish 3, three young girls, a new husband.
        assert_eq!(
            decide(&couple((3, 3, 3), (0, 0), 0, true)),
            Err(Held::Fulfilled)
        );
        assert_eq!(
            decide(&couple((3, 3, 3), (2, 2), 0, true)),
            Err(Held::Fulfilled)
        );
        assert_eq!(
            decide(&couple((3, 3, 3), (2, 1), 0, false)),
            Ok(Opening::His)
        );
        // His slot is gone once the pair has a birth, or once she is one over.
        assert_eq!(
            decide(&couple((3, 4, 4), (2, 1), 0, false)),
            Err(Held::Fulfilled)
        );
        assert_eq!(
            decide(&couple((3, 3, 3), (2, 1), 1, false)),
            Err(Held::Fulfilled)
        );
        // A wish of 0 never opens his slot.
        assert_eq!(
            decide(&couple((0, 0, 0), (4, 0), 0, true)),
            Err(Held::Fulfilled)
        );
        // Four children, the eldest 18, her wish 4: open by the grown child, if she drew so.
        assert_eq!(
            decide(&couple((4, 4, 3), (0, 0), 4, true)),
            Ok(Opening::Grown)
        );
        assert_eq!(
            decide(&couple((4, 4, 3), (0, 0), 4, false)),
            Err(Held::Fulfilled)
        );
        // Twins finished one over a wish of 3: not below 3 + 1.
        assert_eq!(
            decide(&couple((3, 4, 3), (0, 0), 4, true)),
            Err(Held::Fulfilled)
        );
        // Below 3 births a grown child does not reopen her.
        assert_eq!(
            decide(&couple((2, 2, 1), (0, 0), 2, true)),
            Err(Held::Fulfilled)
        );
        // The order of the page: the end of fertility, then pregnancy, then the pause.
        let held = |c: Couple| decide(&c);
        let mut c = couple((3, 2, 2), (0, 0), 0, false);
        c.paused = true;
        assert_eq!(held(c), Err(Held::Pause));
        c.pregnant = true;
        assert_eq!(held(c), Err(Held::Pregnant));
        c.her_age = 50.0;
        assert_eq!(held(c), Err(Held::FertilityOver));
        // Without the extra child the rule is her wish alone.
        let mut c = couple((3, 3, 0), (4, 0), 0, true);
        c.rule = None;
        assert_eq!(held(c), Err(Held::Fulfilled));
        c.her_life = 2;
        assert_eq!(held(c), Ok(Opening::Hers));
    }

    /// Who is young: a town child until the first month start of its 18 whole years (dead or
    /// not), an arrival from the first month start after its arrival, a pool person who never
    /// came always; the candidates of an opening are those month starts.
    #[test]
    fn young_children_and_the_month_starts_they_open() {
        let y = YEAR.as_secs();
        let m = M;
        let key =
            |n: u64| LineageKey::founder(&crate::data_id::DataId::new("test:kin").expect("id"), n);
        let town = |born: i64| Child {
            born: t(born),
            arrived: None,
            mother: Some(key(1)),
            father: Some(key(2)),
        };
        let arrival = |born: i64, arrived: i64| Child {
            born: t(born),
            arrived: Some(t(arrived)),
            mother: Some(key(1)),
            father: Some(key(3)),
        };
        let hers = [town(0), town(13 * m), arrival(-30 * y, 20 * y)];
        // At 18 years after the first birth the eldest leaves; the arrival counts only after
        // the first month start after its arrival.
        assert_eq!(her_young(4, &hers, t(18 * y - 1), 18), 4);
        assert_eq!(her_young(4, &hers, t(18 * y), 18), 3);
        assert_eq!(her_young(4, &hers, t(20 * y), 18), 2);
        assert_eq!(her_young(4, &hers, t(20 * y + m), 18), 1);
        // His: an arrival that has not come counts, one born after `t` does not.
        let his = [town(0), arrival(-30 * y, 20 * y), town(30 * y)];
        assert_eq!(his_young(&his, 2, t(19 * y), 18), 3);
        assert_eq!(his_young(&his, 2, t(20 * y + m), 18), 2);
        assert_eq!(his_young(&his, 0, t(30 * y), 18), 1);
        // The pair's births: her town children naming him, and the pool's members of both.
        assert_eq!(ours(&hers, key(2), 0, t(13 * m)), 2);
        assert_eq!(ours(&hers, key(2), 0, t(13 * m - 1)), 1);
        assert_eq!(ours(&hers, key(3), 1, t(40 * y)), 1);
        let rule = Some(ExtraChild {
            count_until_years: 18,
            extra: 1,
            grown_from: 3,
            grown_chance: 0.2,
        });
        assert_eq!(
            candidates(t(5 * y), &[&hers], rule),
            vec![t(5 * y), t(18 * y), t(18 * y + 13 * m), t(20 * y + m)]
        );
        assert_eq!(candidates(t(5 * y), &[&hers], None), vec![t(5 * y)]);
    }

    /// The end of her fertility: the end of the decline of 5.6, or of the window.
    #[test]
    fn the_end_of_fertility() {
        let window = Curves::Window {
            from: 18.0,
            until: 45.0,
        };
        assert_eq!(fertility_end(&window), 45.0);
        let data = crate::data::testing::data();
        assert_eq!(fertility_end(&data.genetics().fertility), 50.0);
    }
}
