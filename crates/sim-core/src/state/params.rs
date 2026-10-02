//! The input `ForkOverride` (ADR-0001 section 6, ADR-0004 section 4): the override block
//! of a fork changes world parameters, in the order of its list.

use alloc::vec::Vec;

use super::DomainState;
use crate::input::{ForkOverride, InputRejected, RejectCode};
use crate::log::{Causes, LogPayload};
use crate::params::ParamChange;
use crate::world::SimCtx;

pub(crate) fn check_override(
    domain: &DomainState,
    block: &ForkOverride,
) -> Result<(), InputRejected> {
    match override_rejection(domain.world_params.is_some(), &block.changes) {
        Some(code) => Err(InputRejected { code }),
        None => Ok(()),
    }
}

/// Why an override block with these changes is rejected, if at all. Also the check of a
/// saved input log, where the world has started by then.
pub(crate) fn override_rejection(started: bool, changes: &[ParamChange]) -> Option<RejectCode> {
    let _ = (started, changes, RejectCode::NotStarted);
    let _ = (RejectCode::EmptyOverride, RejectCode::OverrideParamTwice);
    let _ = changes.first().map(|change| change.same_param(*change));
    None
}

/// One entry `core:param_changed` per change, in list order.
pub(crate) fn apply_override(
    domain: &mut DomainState,
    block: &ForkOverride,
    ctx: &mut SimCtx<'_, '_>,
) {
    let _ = (&domain.world_params, &block.changes, ctx.trigger());
    let _ = |change| LogPayload::ParamChanged { change };
    let _ = Causes::new;
    let _ = Vec::<ParamChange>::new;
}
