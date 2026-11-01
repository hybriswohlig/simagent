//! The input `ForkOverride` (ADR-0001 section 6, ADR-0004 section 4): the override block
//! of a fork changes world parameters, in the order of its list. The world rule
//! `SetWorldParam` (ADR-0023 section 2) changes one, at a stop of the run, the same way;
//! a shock (`TriggerShock`, section 3) sets the nutrition index of the years of its
//! template.

use alloc::vec::Vec;

use super::{DomainState, death, onset};
use crate::data::{Data, ShockTemplate};
use crate::data_id::DataId;
use crate::input::{
    ForkOverride, InputRecord, InputRejected, RejectCode, SetWorldParam, TriggerShock,
};
use crate::log::{Cause, Causes, LogPayload};
use crate::params::{NutritionYear, ParamChange, WorldParams};
use crate::time::{SimDuration, SimTime, calendar, year_start};
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

/// Whether an input stamped `stamp` has no next second to act in: the last second of
/// time (ADR-0023 section 1).
pub(crate) fn end_of_time(stamp: SimTime) -> bool {
    stamp.checked_add(SimDuration::from_secs(1)).is_none()
}

/// Why a block stamped `stamp` is rejected in a started world, if at all: at the last
/// second of time it cannot act (ADR-0023 section 1); it acts from the next second on and
/// sets no year of the nutrition index before the year of that second (ADR-0013
/// section 2).
pub(crate) fn block_rejection(block: &ForkOverride, stamp: SimTime) -> Option<RejectCode> {
    if end_of_time(stamp) {
        return Some(RejectCode::EndOfTime);
    }
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

/// Why the world rule `change` stamped `stamp` is rejected in a started world with the
/// parameters `params`, if at all (ADR-0023 sections 1 and 2): at the last second of time
/// it cannot act; in phase 1 only the switch of the hereditary diseases is set at run
/// time; the value the switch has is no change.
pub(crate) fn set_rejection(
    change: ParamChange,
    params: &WorldParams,
    stamp: SimTime,
) -> Option<RejectCode> {
    if end_of_time(stamp) {
        return Some(RejectCode::EndOfTime);
    }
    match change {
        ParamChange::NutritionYear(_) => Some(RejectCode::ParamNotSettable),
        ParamChange::HereditaryDiseases(on) => {
            (on == params.hereditary_diseases).then_some(RejectCode::ParamUnchanged)
        }
    }
}

pub(crate) fn check_set(
    domain: &DomainState,
    closed_through: SimTime,
    rule: &SetWorldParam,
) -> Result<(), InputRejected> {
    let code = match domain.world_params.as_ref() {
        None => Some(RejectCode::NotStarted),
        Some(params) => set_rejection(rule.change, params, closed_through),
    };
    match code {
        Some(code) => Err(InputRejected { code }),
        None => Ok(()),
    }
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
    for change in &block.changes {
        let causes = Causes::new(ctx.trigger());
        apply_change(domain, data, *change, causes, ctx);
    }
    let on = (block.changes.iter())
        .any(|change| matches!(change, ParamChange::HereditaryDiseases(true)));
    if on {
        onset::on_switch_on(domain, data, history, ctx);
    }
}

/// The world rule: its one change as in an override block, and turning the switch on
/// writes the onsets nothing could report yet (path 3, ADR-0023 section 2).
pub(crate) fn apply_set(
    domain: &mut DomainState,
    data: &Data,
    rule: &SetWorldParam,
    history: &[InputRecord],
    ctx: &mut SimCtx<'_, '_>,
) {
    let causes = Causes::new(ctx.trigger());
    apply_change(domain, data, rule.change, causes, ctx);
    if matches!(rule.change, ParamChange::HereditaryDiseases(true)) {
        onset::on_switch_on(domain, data, history, ctx);
    }
}

/// The calendar years a shock of `years` years sets when it acts in the second `next`:
/// from the first that begins at or after `next` (ADR-0023 section 3, G8), first and last.
pub(crate) fn shock_years(next: SimTime, years: u32) -> (i64, i64) {
    let year = calendar(next).year;
    let first = if year_start(year) == next {
        year
    } else {
        year + 1
    };
    (first, first + i64::from(years) - 1)
}

/// Why the shock `shock` stamped `stamp` is rejected in a started world with the
/// parameters `params`, if at all (ADR-0023 sections 1 and 3), in this order: at the last
/// second of time it cannot act; a template the data do not hold; a year out of the range
/// of time, checked for every year before any change; a year that already has a hunger
/// (an index below 1).
pub(crate) fn shock_rejection(
    data: &Data,
    params: &WorldParams,
    stamp: SimTime,
    shock: &DataId,
) -> Option<RejectCode> {
    let Some(next) = stamp.checked_add(SimDuration::from_secs(1)) else {
        return Some(RejectCode::EndOfTime);
    };
    // A boost is no shock of this input (ADR-0023, addendum of 2026-11-01).
    let Some((years, index)) = data.shock(shock).and_then(ShockTemplate::hunger) else {
        return Some(RejectCode::ShockUnknown);
    };
    let (first, last) = shock_years(next, years);
    let years = (first..=last).map(|year| NutritionYear { year, index });
    if years.clone().any(|year| !year.in_range()) {
        return Some(RejectCode::NutritionOutOfRange);
    }
    let hungry = |year: i64| {
        (params.nutrition.iter()).any(|entry| entry.year == year && entry.index.get() < 1.0)
    };
    years
        .clone()
        .any(|entry| hungry(entry.year))
        .then_some(RejectCode::ShockOverlaps)
}

pub(crate) fn check_shock(
    domain: &DomainState,
    data: &Data,
    closed_through: SimTime,
    shock: &TriggerShock,
) -> Result<(), InputRejected> {
    let code = match domain.world_params.as_ref() {
        None => Some(RejectCode::NotStarted),
        Some(params) => shock_rejection(data, params, closed_through, &shock.shock),
    };
    match code {
        Some(code) => Err(InputRejected { code }),
        None => Ok(()),
    }
}

/// The shock: its entry, caused by the input, then the index of each of its years as a
/// change, caused by that entry (ADR-0023 section 3).
pub(crate) fn apply_shock(
    domain: &mut DomainState,
    data: &Data,
    shock: &TriggerShock,
    ctx: &mut SimCtx<'_, '_>,
) {
    let (years, index) = (data.shock(&shock.shock))
        .and_then(ShockTemplate::hunger)
        .unwrap_or_else(|| panic!("invariant shock.unknown"));
    let (first, last) = shock_years(ctx.now(), years);
    let entry = ctx.emit(
        LogPayload::ShockTriggered {
            shock: shock.shock.clone(),
            first_year: first,
            last_year: last,
        },
        Vec::new(),
        Causes::new(ctx.trigger()),
    );
    for year in first..=last {
        let change = ParamChange::NutritionYear(NutritionYear { year, index });
        apply_change(domain, data, change, Causes::new(Cause::Event(entry)), ctx);
    }
}

/// One change: the new parameters, the entry `core:param_changed` with `causes`, and, if
/// the switch flips, the deaths of the exposed sims planned again by that entry.
fn apply_change(
    domain: &mut DomainState,
    data: &Data,
    change: ParamChange,
    causes: Causes,
    ctx: &mut SimCtx<'_, '_>,
) {
    let before = domain
        .world_params
        .clone()
        .unwrap_or_else(|| panic!("invariant override.before_start"));
    let was = before.hereditary_diseases;
    let params = before.with(change);
    let on = params.hereditary_diseases;
    domain.world_params = Some(params);
    let entry = ctx.emit(LogPayload::ParamChanged { change }, Vec::new(), causes);
    if on != was {
        death::switch_diseases(domain, data, on, entry, ctx);
    }
}
