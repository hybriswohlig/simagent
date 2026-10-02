//! The input `ForkOverride` (ADR-0001 section 6, ADR-0004 section 4): the override block
//! of a fork changes world parameters, in the order of its list.

use alloc::vec::Vec;

use super::DomainState;
use crate::input::{ForkOverride, InputRejected, RejectCode};
use crate::log::{Causes, LogPayload};
use crate::world::SimCtx;

pub(crate) fn check_override(
    domain: &DomainState,
    block: &ForkOverride,
) -> Result<(), InputRejected> {
    let code = if domain.world_params.is_none() {
        Some(RejectCode::NotStarted)
    } else {
        block.rejection()
    };
    match code {
        Some(code) => Err(InputRejected { code }),
        None => Ok(()),
    }
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
