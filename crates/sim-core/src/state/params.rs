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
    if !started {
        return Some(RejectCode::NotStarted);
    }
    // An empty block would take a place in the input log and make a fork without
    // override differ from its parent.
    if changes.is_empty() {
        return Some(RejectCode::EmptyOverride);
    }
    let twice = changes.iter().enumerate().any(|(index, change)| {
        changes[..index]
            .iter()
            .any(|earlier| earlier.same_param(*change))
    });
    if twice {
        return Some(RejectCode::OverrideParamTwice);
    }
    None
}

/// One entry `core:param_changed` per change, in list order.
pub(crate) fn apply_override(
    domain: &mut DomainState,
    block: &ForkOverride,
    ctx: &mut SimCtx<'_, '_>,
) {
    let mut params = domain
        .world_params
        .unwrap_or_else(|| panic!("invariant override.before_start"));
    for change in &block.changes {
        params = params.with(*change);
        ctx.emit(
            LogPayload::ParamChanged { change: *change },
            Vec::new(),
            Causes::new(ctx.trigger()),
        );
    }
    domain.world_params = Some(params);
}
