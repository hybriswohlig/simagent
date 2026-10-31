//! Partnerships (ADR-0004 sections 1 and 4): the fixed couples of `Start` and the input
//! `ArrangePartnership` pair a living woman and a living man. A partnership ends with the
//! first death of the two or with their separation, whose time a budget fixes when it
//! forms (ADR-0018 section 3), or the input `SeparatePartnership` (ADR-0023, addendum of
//! 2026-10-31), and stays as history.

use super::{DomainState, EndedBy, Ending, Partnership, Slot, conception, household, market};
use crate::data::Data;
use crate::genome::Sex;
use crate::ids::{EventId, PartnershipId, SimId};
use crate::input::{
    ArrangePartnership, Couple, Founder, InputRejected, RejectCode, SeparatePartnership,
};
use crate::kinship::KinshipCache;
use crate::lineage::LineageKey;
use crate::log::{Cause, Causes, DrawRef, LogPayload, Subject};
use crate::rng::{DrawKey, Draws, SubjectKey, WorldSeed, tags};
use crate::time::{SimDuration, SimTime};
use crate::world::{EventPayload, SimCtx};

pub(crate) fn check_arrange(
    domain: &DomainState,
    data: &Data,
    closed_through: SimTime,
    pair: &ArrangePartnership,
) -> Result<(), InputRejected> {
    // The input acts in the second after the clock: the partnership forms there, and at
    // the last second of time there is none (ADR-0023 section 1).
    let formed = closed_through.checked_add(SimDuration::from_secs(1));
    let code = match (domain.world_seed, formed) {
        (None, _) => Some(RejectCode::NotStarted),
        (Some(_), None) => Some(RejectCode::EndOfTime),
        (Some(_), Some(formed)) => arrange_rejection(domain, pair)
            .or_else(|| kin_or_age_rejection(domain, data, (pair.woman, pair.man), formed)),
    };
    match code {
        Some(code) => Err(InputRejected { code }),
        None => Ok(()),
    }
}

/// Why these two cannot be paired, if at all, in this order: a sim never handed out,
/// one in the archive, not a woman and a man, one in a partnership, one under the market
/// age, too closely related (ADR-0018 section 1).
fn arrange_rejection(domain: &DomainState, pair: &ArrangePartnership) -> Option<RejectCode> {
    pair_rejection(domain, (pair.woman, pair.man)).or_else(|| {
        [pair.woman, pair.man]
            .iter()
            .any(|id| open_partnership(domain, *id).is_some())
            .then_some(RejectCode::AlreadyPartnered)
    })
}

/// Why a woman and a man an input names are no pair to act on, if at all, in this order:
/// a sim never handed out, one in the archive, not a woman and a man.
fn pair_rejection(domain: &DomainState, (woman, man): (SimId, SimId)) -> Option<RejectCode> {
    let both = [woman, man];
    let known = |id: &SimId| domain.sims.contains_key(id) || domain.archive.contains_key(id);
    if !both.iter().all(known) {
        return Some(RejectCode::UnknownSim);
    }
    if !both.iter().all(|id| domain.sims.contains_key(id)) {
        return Some(RejectCode::SimNotAlive);
    }
    let sex = |id: &SimId| domain.sims.get(id).map(|sim| sim.sex);
    if sex(&woman) != Some(Sex::Female) || sex(&man) != Some(Sex::Male) {
        return Some(RejectCode::PartnersOfWrongSex);
    }
    None
}

/// Pure check of `SeparatePartnership` (ADR-0023, addendum of 2026-10-31): a started world
/// with a second after the clock, a known and living woman and man as for a pairing, then
/// their open partnership with each other.
pub(crate) fn check_separate(
    domain: &DomainState,
    closed_through: SimTime,
    pair: &SeparatePartnership,
) -> Result<(), InputRejected> {
    let next = closed_through.checked_add(SimDuration::from_secs(1));
    let code = match (domain.world_seed, next) {
        (None, _) => Some(RejectCode::NotStarted),
        (Some(_), None) => Some(RejectCode::EndOfTime),
        (Some(_), Some(_)) => pair_rejection(domain, (pair.woman, pair.man)).or_else(|| {
            couple_of(domain, (pair.woman, pair.man))
                .is_none()
                .then_some(RejectCode::NotPartners)
        }),
    };
    match code {
        Some(code) => Err(InputRejected { code }),
        None => Ok(()),
    }
}

/// The open partnership of `woman` and `man` with each other, if any.
fn couple_of(domain: &DomainState, (woman, man): (SimId, SimId)) -> Option<PartnershipId> {
    let id = open_partnership(domain, woman)?;
    (open_partnership(domain, man) == Some(id)).then_some(id)
}

/// The player parts the couple now (ADR-0023, addendum of 2026-10-31): a separation as in
/// ADR-0018 section 3, with the input as its only cause; its planned separation is struck.
pub(crate) fn apply_separate(
    domain: &mut DomainState,
    pair: &SeparatePartnership,
    ctx: &mut SimCtx<'_, '_>,
) {
    let Some(id) = couple_of(domain, (pair.woman, pair.man)) else {
        panic!("invariant separate.checked");
    };
    if let Some(handle) = domain.pending.remove(&(pair.woman, Slot::Separation)) {
        ctx.cancel(handle);
    }
    let causes = Causes::new(ctx.trigger());
    separate(domain, id, causes, ctx);
}

/// Why two living sims of the right sexes and without a partnership cannot be paired now,
/// if at all: one under the market age, then in direct line, then too closely related
/// (ADR-0018 sections 1 and 8).
pub(crate) fn kin_or_age_rejection(
    domain: &DomainState,
    data: &Data,
    (woman, man): (SimId, SimId),
    now: SimTime,
) -> Option<RejectCode> {
    let age = market::market_age(data);
    let young = [woman, man].into_iter().any(|sim| {
        domain
            .sims
            .get(&sim)
            .is_none_or(|person| !market::of_age(person.born, now, age))
    });
    if young {
        return Some(RejectCode::PartnerTooYoung);
    }
    if market::direct_line(domain, data, woman, man) {
        return Some(RejectCode::PartnersInDirectLine);
    }
    let limit = data.demography().market.kinship_limit;
    let phi = market::phi_with(&mut KinshipCache::new(), domain, data, woman, man);
    (phi >= limit).then_some(RejectCode::PartnersTooCloselyRelated)
}

/// Why the couples of `Start` are rejected, if at all, in this order: a place beyond
/// the founders, not a woman and a man, a founder in two couples, a founder under the
/// market age `age` (in whole years) at the start (ADR-0018 section 1). Founders are
/// unrelated, so no couple is too closely related.
pub(crate) fn couples_rejection(
    founders: &[Founder],
    couples: &[Couple],
    age: i64,
) -> Option<RejectCode> {
    let founder = |place: u32| usize::try_from(place).ok().and_then(|i| founders.get(i));
    if couples
        .iter()
        .any(|couple| founder(couple.woman).is_none() || founder(couple.man).is_none())
    {
        return Some(RejectCode::UnknownFounder);
    }
    let sex = |place: u32| founder(place).map(|founder| founder.sex);
    if couples
        .iter()
        .any(|couple| sex(couple.woman) != Some(Sex::Female) || sex(couple.man) != Some(Sex::Male))
    {
        return Some(RejectCode::PartnersOfWrongSex);
    }
    // A woman and a man are never the same founder: their sexes differ.
    let twice = couples.iter().enumerate().any(|(index, couple)| {
        couples[..index]
            .iter()
            .any(|earlier| earlier.woman == couple.woman || earlier.man == couple.man)
    });
    if twice {
        return Some(RejectCode::AlreadyPartnered);
    }
    let young = couples.iter().any(|couple| {
        [couple.woman, couple.man].into_iter().any(|place| {
            founder(place).is_some_and(|f| !market::of_age(f.born, SimTime::START, age))
        })
    });
    young.then_some(RejectCode::PartnerTooYoung)
}

pub(crate) fn apply_arrange(
    domain: &mut DomainState,
    data: &Data,
    pair: &ArrangePartnership,
    ctx: &mut SimCtx<'_, '_>,
) {
    let causes = Causes::new(ctx.trigger());
    pair_and_move(domain, data, (pair.woman, pair.man), causes, ctx);
}

/// A partnership formed now after `Start` (an input or the market): `pair_up_with`, then
/// the couple founds its household (ADR-0018 section 4).
pub(crate) fn pair_and_move(
    domain: &mut DomainState,
    data: &Data,
    couple: (SimId, SimId),
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let (_, entry) = pair_up_with(domain, data, couple, causes, ctx);
    household::pair_moves(domain, couple, entry, ctx);
}

/// A new partnership of two living sims, formed now, with its creation entry caused
/// by what runs (the input). Her monthly attempts begin by that entry, at the first
/// month start after now (ADR-0017 section 4); then its separation is planned by it.
pub(crate) fn pair_up(
    domain: &mut DomainState,
    data: &Data,
    woman: SimId,
    man: SimId,
    ctx: &mut SimCtx<'_, '_>,
) -> PartnershipId {
    let causes = Causes::new(ctx.trigger());
    pair_up_with(domain, data, (woman, man), causes, ctx).0
}

/// As `pair_up`, with the causes of the creation entry given (the market names its rule
/// and the draw of the chosen candidate, ADR-0018 section 1).
pub(crate) fn pair_up_with(
    domain: &mut DomainState,
    data: &Data,
    (woman, man): (SimId, SimId),
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) -> (PartnershipId, EventId) {
    let (id, entry) = ctx.create_partnership(woman, man, causes);
    domain.open.insert(woman, id);
    domain.open.insert(man, id);
    domain.partnerships.insert(
        id,
        Partnership {
            woman,
            man,
            formed: ctx.now(),
            ended: None,
        },
    );
    let start = conception::series_after_pairing(ctx.now());
    conception::plan_attempts(domain, data, woman, entry, start, ctx);
    plan_separation(domain, data, (id, woman, man), entry, ctx);
    (id, entry)
}

/// The address of the budget of the separation of her partnership number `k` with him
/// (ADR-0018 section 3).
pub(crate) fn separation_key(her: LineageKey, k: u32, his: LineageKey) -> DrawKey {
    DrawKey {
        tag: tags::SEPARATION_BUDGET,
        subject: SubjectKey::from(her),
        occasion: u64::from(k),
        detail: his.get(),
    }
}

/// When her partnership number `k` with him, formed at `formed`, separates if both live:
/// `B / λ` years later, rounded up to the second and at least one second after the
/// pairing (ADR-0018 section 3). `None` without separation (`λ` = 0) or beyond the clock.
pub(crate) fn separation_due(
    data: &Data,
    seed: u64,
    formed: SimTime,
    (her, k, his): (LineageKey, u32, LineageKey),
) -> Option<SimTime> {
    let rate = data.demography().separation_rate;
    if rate == 0.0 {
        return None;
    }
    let budget = Draws::new(WorldSeed::new(seed)).exponential(separation_key(her, k, his), 1.0);
    let after = SimDuration::from_years_ceil(budget / rate)?;
    formed.checked_add(after.max(SimDuration::from_secs(1)))
}

/// Plans the separation of the partnership `id` of `woman` and `man`, formed now, by its
/// creation entry `by`. Her count of partnerships gives the address of the budget and
/// then grows by one.
fn plan_separation(
    domain: &mut DomainState,
    data: &Data,
    (id, woman, man): (PartnershipId, SimId, SimId),
    by: EventId,
    ctx: &mut SimCtx<'_, '_>,
) {
    let seed = domain
        .world_seed
        .unwrap_or_else(|| panic!("invariant partnership.before_start"));
    let his = lineage_key(domain, man);
    let Some(her) = domain.sims.get_mut(&woman) else {
        panic!("invariant unknown_entity_id: sim {}", woman.get());
    };
    let k = her.partnerships;
    her.partnerships = k
        .checked_add(1)
        .unwrap_or_else(|| panic!("invariant partnership.count"));
    let address = (her.lineage_key, k, his);
    if let Some(due) = separation_due(data, seed, ctx.now(), address) {
        let handle = ctx.schedule(due, EventPayload::Separation { partnership: id }, by);
        domain.pending.insert((woman, Slot::Separation), handle);
    }
}

/// The lineage key of a living sim.
fn lineage_key(domain: &DomainState, sim: SimId) -> LineageKey {
    domain
        .sims
        .get(&sim)
        .map(|person| person.lineage_key)
        .unwrap_or_else(|| panic!("invariant unknown_entity_id: sim {}", sim.get()))
}

/// The budget of the separation of the partnership `id` ran out while both live: it ends
/// now by separation, and with it her attempts; a running pregnancy stays (ADR-0018
/// section 3). The entry names the plan and the draw of the budget as its causes.
pub(crate) fn on_separation(domain: &mut DomainState, id: PartnershipId, ctx: &mut SimCtx<'_, '_>) {
    let Some(partnership) = domain.partnerships.get(&id) else {
        panic!("invariant unknown_entity_id: partnership {}", id.get());
    };
    let (woman, man) = (partnership.woman, partnership.man);
    // The event that runs right now is no longer pending.
    domain.pending.remove(&(woman, Slot::Separation));
    let (her, his) = (lineage_key(domain, woman), lineage_key(domain, man));
    // Her partnerships end with this one: it is her last.
    let k = domain
        .sims
        .get(&woman)
        .and_then(|person| person.partnerships.checked_sub(1))
        .unwrap_or_else(|| panic!("invariant partnership.count"));
    let draw = separation_key(her, k, his);
    let causes = Causes::new(ctx.trigger()).with(Cause::Random(DrawRef::from(draw)));
    separate(domain, id, causes, ctx);
}

/// The partnership `id` ends now by separation, its entry with `causes`, and with it her
/// attempts; a running pregnancy stays, and the couple moves apart (ADR-0018 section 3).
fn separate(domain: &mut DomainState, id: PartnershipId, causes: Causes, ctx: &mut SimCtx<'_, '_>) {
    let now = ctx.now();
    let Some(partnership) = domain.partnerships.get_mut(&id) else {
        panic!("invariant unknown_entity_id: partnership {}", id.get());
    };
    partnership.ended = Some(Ending {
        at: now,
        by: EndedBy::Separation,
    });
    let (woman, man) = (partnership.woman, partnership.man);
    domain.open.remove(&woman);
    domain.open.remove(&man);
    let separated = ctx.emit(
        LogPayload::Separated { partnership: id },
        alloc::vec![Subject::Sim(woman), Subject::Sim(man)],
        causes,
    );
    conception::stop_attempts(domain, woman, ctx);
    household::separation_moves(domain, (woman, man), separated, ctx);
}

/// The partnership of `sim` that has not ended, if any; a sim has at most one. Read from
/// the index of open partnerships (ADR-0018 section 2).
pub(crate) fn open_partnership(domain: &DomainState, sim: SimId) -> Option<PartnershipId> {
    domain.open.get(&sim).copied()
}

/// `sim` died or emigrated now (`by`): its open partnership ends in this second, and with
/// it her attempts and its separation.
pub(crate) fn end_with(
    domain: &mut DomainState,
    sim: SimId,
    by: EndedBy,
    ctx: &mut SimCtx<'_, '_>,
) {
    let now: SimTime = ctx.now();
    if let Some(id) = open_partnership(domain, sim)
        && let Some(partnership) = domain.partnerships.get_mut(&id)
    {
        partnership.ended = Some(Ending { at: now, by });
        let (woman, man) = (partnership.woman, partnership.man);
        domain.open.remove(&woman);
        domain.open.remove(&man);
        conception::stop_attempts(domain, woman, ctx);
        if let Some(handle) = domain.pending.remove(&(woman, Slot::Separation)) {
            ctx.cancel(handle);
        }
    }
}
