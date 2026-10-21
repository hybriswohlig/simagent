//! The input `ForkOverride` (ADR-0001 section 6, ADR-0004 section 4): the override block
//! of a fork changes world parameters, in the order of its list.

use alloc::vec::Vec;

use super::{DomainState, death, onset};
use crate::data::Data;
use crate::input::{ForkOverride, InputRecord, InputRejected, RejectCode};
use crate::log::{Causes, LogPayload};
use crate::params::ParamChange;
use crate::time::{SimDuration, SimTime, calendar};
use crate::world::SimCtx;

pub(crate) fn check_override(
    domain: &DomainState,
    closed_through: SimTime,
    block: &ForkOverride,
) -> Result<(), InputRejected> {
    let code = if domain.world_params.is_none() {
        Some(RejectCode::NotStarted)
    } else {
        block_rejection(block, closed_through)
    };
    match code {
        Some(code) => Err(InputRejected { code }),
        None => Ok(()),
    }
}

/// Why a block stamped `stamp` is rejected in a started world, if at all: it acts from
/// the next second on and sets no year of the nutrition index before the year of that
/// second (ADR-0013 section 2). At the last second of time there is no next second, and
/// so no year left to set.
pub(crate) fn block_rejection(block: &ForkOverride, stamp: SimTime) -> Option<RejectCode> {
    block.rejection().or_else(|| {
        let fork_year = stamp
            .checked_add(SimDuration::from_secs(1))
            .map(|next| calendar(next).year);
        let past = block.changes.iter().any(|change| match change {
            ParamChange::NutritionYear(entry) => fork_year.is_none_or(|year| entry.year < year),
            ParamChange::HereditaryDiseases(_) => false,
        });
        past.then_some(RejectCode::NutritionYearPast)
    })
}

/// One entry `core:param_changed` per change, in list order. A change that flips the
/// switch of the hereditary diseases plans the deaths of the exposed sims again, by
/// its entry; one that keeps a value changes nothing else. A block that sets the switch on
/// then writes the onsets nothing could report yet (path 3; `history` the inputs before it).
pub(crate) fn apply_override(
    domain: &mut DomainState,
    data: &Data,
    block: &ForkOverride,
    history: &[InputRecord],
    ctx: &mut SimCtx<'_, '_>,
) {
    let mut params = domain
        .world_params
        .clone()
        .unwrap_or_else(|| panic!("invariant override.before_start"));
    for change in &block.changes {
        let before = params.hereditary_diseases;
        params = params.with(*change);
        domain.world_params = Some(params.clone());
        let entry = ctx.emit(
            LogPayload::ParamChanged { change: *change },
            Vec::new(),
            Causes::new(ctx.trigger()),
        );
        if params.hereditary_diseases != before {
            death::switch_diseases(domain, data, params.hereditary_diseases, entry, ctx);
        }
    }
    let on = (block.changes.iter())
        .any(|change| matches!(change, ParamChange::HereditaryDiseases(true)));
    if on {
        onset::on_switch_on(domain, data, history, ctx);
    }
}
