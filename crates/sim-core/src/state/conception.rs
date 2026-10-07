//! Conception (ADR-0001 scenario `fork_no_density`, ADR-0004 section 7, B18): the woman
//! of an open partnership tries once a month while she can conceive. A successful draw
//! fixes the child at once: its lineage key from both parents and her conception number,
//! one meiosis per parent at the dummy locus and the sex chromosome of the father. The
//! pregnancy is state.

use alloc::vec;

use super::{DomainState, Pregnancy, Slot, partnership};
use crate::data::Data;
use crate::data_id::Code;
use crate::genome::meiosis::Zygote;
use crate::ids::{EventId, SimId};
use crate::input::Fertility;
use crate::lineage::{GenomeKey, LineageKey};
use crate::log::{Cause, Causes, DrawRef, LogPayload, Subject};
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{MONTH, SimTime, YEAR};
use crate::world::{EventPayload, SimCtx};

/// The second a person born at `born` turns `years` old; `None` beyond the clock.
pub(crate) fn birthday(born: SimTime, years: u32) -> Option<SimTime> {
    born.checked_add(YEAR.checked_mul(i64::from(years))?)
}

/// The first month start at or after `t`; `None` beyond the clock.
fn month_start_at_or_after(t: SimTime) -> Option<SimTime> {
    let month = MONTH.as_secs();
    let index = t.as_secs().div_euclid(month) + i64::from(t.as_secs().rem_euclid(month) != 0);
    SimTime::try_from(index.checked_mul(month)?).ok()
}

/// When the attempts of a woman born at `born` begin if they are planned at `from`: the
/// first month start at or after `from` and her `from_years` birthday, `None` if she is
/// `until_years` old by then (or that lies beyond the clock).
pub(crate) fn first_attempt(
    fertility: &Fertility,
    born: SimTime,
    from: SimTime,
) -> Option<SimTime> {
    let old_enough = birthday(born, fertility.from_years)?;
    let too_old = birthday(born, fertility.until_years)?;
    let first = month_start_at_or_after(from.max(old_enough))?;
    (first < too_old).then_some(first)
}

/// Plans the monthly attempts of `woman` from `from` on (her pairing, or the second
/// after a birth), by `by`; nothing if she is pregnant or cannot conceive any more.
pub(crate) fn plan_attempts(
    domain: &mut DomainState,
    woman: SimId,
    by: EventId,
    from: SimTime,
    ctx: &mut SimCtx<'_, '_>,
) {
    let fertility = domain
        .fertility
        .unwrap_or_else(|| panic!("invariant conception.before_start"));
    let Some(her) = domain.sims.get(&woman) else {
        panic!("invariant unknown_entity_id: sim {}", woman.get());
    };
    if domain.pregnancies.contains_key(&woman) {
        return;
    }
    let Some(first) = first_attempt(&fertility, her.born, from) else {
        return;
    };
    let payload = EventPayload::ConceptionAttempt { woman };
    let handle = ctx.schedule_periodic(first, MONTH, payload, by);
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
/// entry; it ends the series if the next month start is her `until_years` birthday or
/// later, so that a pending attempt always lies before it.
pub(crate) fn on_attempt(
    domain: &mut DomainState,
    data: &Data,
    woman: SimId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let fertility = domain
        .fertility
        .unwrap_or_else(|| panic!("invariant conception.before_start"));
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant conception.before_start"));
    let now = ctx.now();
    let Some(her) = domain.sims.get(&woman) else {
        panic!("invariant unknown_entity_id: sim {}", woman.get());
    };
    let too_old = birthday(her.born, fertility.until_years);
    let Some(man) = partnership::open_partnership(domain, woman)
        .and_then(|id| domain.partnerships.get(&id))
        .map(|partnership| partnership.man)
    else {
        panic!("invariant conception.without_partnership");
    };
    let Some(him) = domain.sims.get(&man) else {
        panic!("invariant unknown_entity_id: sim {}", man.get());
    };
    let month = month_index(now).unwrap_or_else(|| panic!("invariant conception.before_start"));
    let draw = attempt_key(her.lineage_key, him.lineage_key, month);
    if !Draws::new(WorldSeed::new(seed)).bernoulli(draw, fertility.per_month.get()) {
        let next = now.checked_add(MONTH);
        if next.is_none_or(|next| too_old.is_none_or(|too_old| next >= too_old)) {
            stop_attempts(domain, woman, ctx);
        }
        return;
    }
    stop_attempts(domain, woman, ctx);
    conceive(domain, data, seed, &fertility, woman, man, draw, ctx);
}

/// She conceives her next child with him now: the entries of the conception and of the
/// two meioses (genetics specification 2.3), and the pregnancy.
#[expect(clippy::too_many_arguments, reason = "the facts of one conception")]
fn conceive(
    domain: &mut DomainState,
    data: &Data,
    seed: u64,
    fertility: &Fertility,
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
    let zygote = Zygote::of(&draws, &data.genetics(), genome, &her.genome, &his_genome);
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
    let due = birth_of(fertility, now).unwrap_or_else(|| panic!("invariant time.out_of_range"));
    let handle = ctx.schedule(due, EventPayload::Birth { mother: woman }, conception);
    domain.pending.insert((woman, Slot::Birth), handle);
}

/// When a child conceived at `conceived` is born: `pregnancy_months` months later.
pub(crate) fn birth_of(fertility: &Fertility, conceived: SimTime) -> Option<SimTime> {
    conceived.checked_add(MONTH.checked_mul(i64::from(fertility.pregnancy_months))?)
}
