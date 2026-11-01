//! Conception (genetics specification 5.6 and 11.3, ADR-0017 section 4, B18): the woman
//! of an open partnership tries once a month while she has fewer live births than she
//! wishes, is not pregnant and her fertility has not ended. The chance is the adult rate
//! times her factor from her menarche and his from his puberty timing, both at the moment
//! of the attempt. A successful draw fixes the child at once: its lineage key from both
//! parents and her conception number, one meiosis per parent and the sex chromosome of
//! the father. The pregnancy is state; menarche and wish are pure functions of the seed.

use alloc::vec;

use super::{DomainState, Pregnancy, Slot, Unborn, disease, partnership, together};
use crate::data::Data;
use crate::data_id::Code;
use crate::data_id::DataId;
use crate::fertility::{self, Curves};
use crate::genome::Genome;
use crate::genome::meiosis::Zygote;
use crate::ids::{EventId, PartnershipId, SimId};
use crate::lineage::{GenomeKey, LineageKey};
use crate::log::{Cause, Causes, DrawRef, LogPayload, Subject};
use crate::phenotype::{self, Parents};
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{MONTH, SimDuration, SimTime};
use crate::world::{EventPayload, SimCtx};

/// The first month start after `t`; `None` beyond the clock.
pub(crate) fn month_start_after(t: SimTime) -> Option<SimTime> {
    let index = t.as_secs().div_euclid(MONTH.as_secs()).checked_add(1)?;
    SimTime::try_from(index.checked_mul(MONTH.as_secs())?).ok()
}

/// The first month start at or after `t`; `None` beyond the clock.
fn month_start_at_or_after(t: SimTime) -> Option<SimTime> {
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

/// Her wish for children (11.3): a pure function of the seed and her lineage key.
pub(crate) fn wish(data: &Data, seed: u64, her: LineageKey) -> u32 {
    let key = DrawKey {
        tag: tags::FERTILITY_WISH,
        subject: SubjectKey::from(her),
        occasion: 0,
        detail: 0,
    };
    let u = Draws::new(WorldSeed::new(seed)).unit(key);
    let fertility = &data.demography().fertility;
    fertility::wish(u, &fertility.wish_shape, fertility.wish_tilt)
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

/// Whether a series beginning at `start` belongs to her: fewer live births than her wish
/// and her fertility not ended at `start`.
pub(crate) fn may_begin(
    data: &Data,
    seed: u64,
    born: SimTime,
    key: LineageKey,
    births: u32,
    start: SimTime,
) -> bool {
    births < wish(data, seed, key)
        && disease::age(born, start) < fertility_end(&data.genetics().fertility)
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

/// Plans the monthly attempts of `woman` from `start` (a month start: after her pairing
/// or after a birth), by `by`; nothing if she is pregnant, has her wish, or her fertility
/// has ended by then.
pub(crate) fn plan_attempts(
    domain: &mut DomainState,
    data: &Data,
    woman: SimId,
    by: EventId,
    start: Option<SimTime>,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(her) = domain.sims.get(&woman) else {
        panic!("invariant unknown_entity_id: sim {}", woman.get());
    };
    let Some(start) = start else {
        return;
    };
    let life = domain.seeds(data, woman).life;
    if domain.pregnancies.contains_key(&woman)
        || !may_begin(data, life, her.born, her.lineage_key, her.births, start)
    {
        return;
    }
    let payload = EventPayload::ConceptionAttempt { woman };
    let handle = ctx.schedule_periodic(start, MONTH, payload, by);
    domain.pending.insert((woman, Slot::Conception), handle);
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
