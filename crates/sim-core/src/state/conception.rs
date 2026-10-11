//! Conception (genetics specification 5.6 and 11.3, ADR-0017 section 4, B18): the woman
//! of an open partnership tries once a month while she has fewer live births than she
//! wishes, is not pregnant and her fertility has not ended. The chance is the adult rate
//! times her factor from her menarche and his from his puberty timing, both at the moment
//! of the attempt. A successful draw fixes the child at once: its lineage key from both
//! parents and her conception number, one meiosis per parent and the sex chromosome of
//! the father. The pregnancy is state; menarche and wish are pure functions of the seed.

use alloc::vec;

use super::{DomainState, Pregnancy, Slot, disease, partnership};
use crate::data::Data;
use crate::data_id::Code;
use crate::fertility::{self, Curves};
use crate::genome::Genome;
use crate::genome::meiosis::Zygote;
use crate::ids::{EventId, SimId};
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
}

/// The puberty timing `θ_i` of a person at `t` (5.2): from the phenotype of the trait the
/// data name, else `θ` of the curve of the sex (11.1: the term drops).
fn theta_i(data: &Data, seed: u64, person: &Person<'_>, t: SimTime) -> f64 {
    let genetics = data.genetics();
    let curve = genetics.growth(person.genome.sex());
    let z_p = data
        .puberty_trait()
        .and_then(|t_p| {
            let age = t.checked_since(person.born)?;
            phenotype::phenotype(
                &Draws::new(WorldSeed::new(seed)),
                data,
                person.genome,
                person.lineage_key,
                person.parents,
                t_p,
                age,
            )
        })
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

/// The chance of her attempt with him at `t` (11.3): the adult rate times her factor from
/// her menarche and his from his puberty timing, both at `t`.
pub(crate) fn chance(
    data: &Data,
    seed: u64,
    her: &Person<'_>,
    him: &Person<'_>,
    t: SimTime,
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
            let delta =
                Draws::new(WorldSeed::new(seed)).normal(key, p.menarche_mean, p.menarche_sd);
            fertility::menarche(theta_i(data, seed, her, t), delta, p)
        }
        Curves::Window { .. } => 0.0,
    };
    let female = fertility::female_factor(disease::age(her.born, t), her_menarche, &curves);
    let male = fertility::male_factor(
        disease::age(him.born, t),
        theta_i(data, seed, him, t),
        &curves,
    );
    fertility::chance(data.demography().fertility.adult_rate, female, male)
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
fn person<'a>(domain: &'a DomainState, id: SimId) -> Person<'a> {
    let Some(sim) = domain.sims.get(&id) else {
        panic!("invariant unknown_entity_id: sim {}", id.get());
    };
    let key_of = |parent: SimId| {
        domain
            .sims
            .get(&parent)
            .map(|p| p.lineage_key)
            .or_else(|| domain.archive.get(&parent).map(|p| p.lineage_key))
            .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", parent.get()))
    };
    Person {
        born: sim.born,
        lineage_key: sim.lineage_key,
        genome: &sim.genome,
        parents: sim.parents.map(|p| Parents {
            mother: key_of(p.mother),
            father: key_of(p.father),
        }),
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
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant conception.before_start"));
    let Some(her) = domain.sims.get(&woman) else {
        panic!("invariant unknown_entity_id: sim {}", woman.get());
    };
    let Some(start) = start else {
        return;
    };
    if domain.pregnancies.contains_key(&woman)
        || !may_begin(data, seed, her.born, her.lineage_key, her.births, start)
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

/// An attempt of the monthly series of `woman`: the draw decides. A failed one writes no
/// entry; it ends the series if her fertility has ended by the next month start, so that
/// no attempt lies at or after its end.
pub(crate) fn on_attempt(
    domain: &mut DomainState,
    data: &Data,
    woman: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant conception.before_start"));
    let now = ctx.now();
    let Some(man) = partnership::open_partnership(domain, woman)
        .and_then(|id| domain.partnerships.get(&id))
        .map(|partnership| partnership.man)
    else {
        panic!("invariant conception.without_partnership");
    };
    let (her, him) = (person(domain, woman), person(domain, man));
    let month = month_index(now).unwrap_or_else(|| panic!("invariant conception.before_start"));
    let draw = attempt_key(her.lineage_key, him.lineage_key, month);
    let p = chance(data, seed, &her, &him, now);
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

/// She conceives her next child with him now: the entries of the conception and of the
/// two meioses (genetics specification 2.3), and the pregnancy.
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
    let child = LineageKey::child(her.lineage_key, his_key, u64::from(number), 0);
    let genome = GenomeKey::of_zygote(child);
    let draws = Draws::new(WorldSeed::new(seed));
    let zygote = Zygote::of(&draws, data, genome, &her.genome, &his_genome);
    domain.lineage.insert(child);
    let conception = ctx.emit(
        LogPayload::Conception { lineage_key: child },
        vec![Subject::Sim(woman), Subject::Sim(man)],
        Causes::new(ctx.trigger()).with(Cause::Random(DrawRef::from(draw))),
    );
    // ADR-0004 section 7: the causes of a meiosis are the conception and the rule; its
    // draws follow from the genome key and the side (ADR-0001).
    let rule =
        Code::new("core:meiosis").unwrap_or_else(|_| panic!("invariant conception.rule_code"));
    for (parent, meiosis) in [(woman, &zygote.mother), (man, &zygote.father)] {
        ctx.emit(
            LogPayload::Meiosis {
                genome_key: genome,
                segments: meiosis.segments.clone(),
                sex_chromosome: meiosis.sex_chromosome,
                mutations: meiosis.mutations.clone(),
            },
            vec![Subject::Sim(parent)],
            Causes::new(Cause::Event(conception)).with(Cause::Rule(rule.clone())),
        );
    }
    let now = ctx.now();
    domain.pregnancies.insert(
        woman,
        Pregnancy {
            father: man,
            lineage_key: child,
            conception: number,
            sex: zygote.genome.sex(),
            genome: zygote.genome,
            conceived: now,
        },
    );
    let due = birth_of(data, now).unwrap_or_else(|| panic!("invariant time.out_of_range"));
    let handle = ctx.schedule(due, EventPayload::Birth { mother: woman }, conception);
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
