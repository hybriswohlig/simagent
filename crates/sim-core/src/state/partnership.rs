//! Partnerships (ADR-0004 sections 1 and 4): the fixed couples of `Start` and the input
//! `ArrangePartnership` pair a living woman and a living man. A partnership ends with the
//! first death of the two and stays as history.

use super::{DomainState, Partnership, conception};
use crate::data::Data;
use crate::genome::Sex;
use crate::ids::{PartnershipId, SimId};
use crate::input::{ArrangePartnership, Couple, Founder, InputRejected, RejectCode};
use crate::log::Causes;
use crate::time::SimTime;
use crate::world::SimCtx;

pub(crate) fn check_arrange(
    domain: &DomainState,
    pair: &ArrangePartnership,
) -> Result<(), InputRejected> {
    let code = if domain.world_seed.is_none() {
        Some(RejectCode::NotStarted)
    } else {
        arrange_rejection(domain, pair)
    };
    match code {
        Some(code) => Err(InputRejected { code }),
        None => Ok(()),
    }
}

/// Why these two cannot be paired, if at all, in this order: a sim never handed out,
/// one in the archive, not a woman and a man, one in a partnership.
fn arrange_rejection(domain: &DomainState, pair: &ArrangePartnership) -> Option<RejectCode> {
    let both = [pair.woman, pair.man];
    let known = |id: &SimId| domain.sims.contains_key(id) || domain.archive.contains_key(id);
    if !both.iter().all(known) {
        return Some(RejectCode::UnknownSim);
    }
    if !both.iter().all(|id| domain.sims.contains_key(id)) {
        return Some(RejectCode::SimNotAlive);
    }
    let sex = |id: &SimId| domain.sims.get(id).map(|sim| sim.sex);
    if sex(&pair.woman) != Some(Sex::Female) || sex(&pair.man) != Some(Sex::Male) {
        return Some(RejectCode::PartnersOfWrongSex);
    }
    if both
        .iter()
        .any(|id| open_partnership(domain, *id).is_some())
    {
        return Some(RejectCode::AlreadyPartnered);
    }
    None
}

/// Why the couples of `Start` are rejected, if at all, in this order: a place beyond
/// the founders, not a woman and a man, a founder in two couples.
pub(crate) fn couples_rejection(founders: &[Founder], couples: &[Couple]) -> Option<RejectCode> {
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
    twice.then_some(RejectCode::AlreadyPartnered)
}

pub(crate) fn apply_arrange(
    domain: &mut DomainState,
    data: &Data,
    pair: &ArrangePartnership,
    ctx: &mut SimCtx<'_, '_>,
) {
    pair_up(domain, data, pair.woman, pair.man, ctx);
}

/// A new partnership of two living sims, formed now, with its creation entry caused
/// by what runs (the input). Her monthly attempts begin by that entry, at the first
/// month start after now (ADR-0017 section 4).
pub(crate) fn pair_up(
    domain: &mut DomainState,
    data: &Data,
    woman: SimId,
    man: SimId,
    ctx: &mut SimCtx<'_, '_>,
) -> PartnershipId {
    let (id, entry) = ctx.create_partnership(woman, man, Causes::new(ctx.trigger()));
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
    id
}

/// The partnership of `sim` that has not ended, if any; a sim has at most one.
pub(crate) fn open_partnership(domain: &DomainState, sim: SimId) -> Option<PartnershipId> {
    domain
        .partnerships
        .iter()
        .find(|(_, partnership)| {
            partnership.ended.is_none() && (partnership.woman == sim || partnership.man == sim)
        })
        .map(|(id, _)| *id)
}

/// `sim` died now: its open partnership ends in this second, and with it her attempts.
pub(crate) fn end_with_death(domain: &mut DomainState, sim: SimId, ctx: &mut SimCtx<'_, '_>) {
    let now: SimTime = ctx.now();
    if let Some(id) = open_partnership(domain, sim)
        && let Some(partnership) = domain.partnerships.get_mut(&id)
    {
        partnership.ended = Some(now);
        let woman = partnership.woman;
        conception::stop_attempts(domain, woman, ctx);
    }
}
