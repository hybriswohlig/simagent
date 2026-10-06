//! The input `ForkOverride` (ADR-0001 section 6, ADR-0004 section 4): the override block
//! of a fork changes world parameters, in the order of its list.

use alloc::vec::Vec;

use super::{DomainState, death};
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

/// One entry `core:param_changed` per change, in list order. A change that flips the
/// switch of the hereditary diseases plans the deaths of the exposed sims again, by
/// its entry; one that keeps a value changes nothing else.
pub(crate) fn apply_override(
    domain: &mut DomainState,
    block: &ForkOverride,
    ctx: &mut SimCtx<'_, '_>,
) {
    let mut params = domain
        .world_params
        .unwrap_or_else(|| panic!("invariant override.before_start"));
    for change in &block.changes {
        let before = params;
        params = params.with(*change);
        domain.world_params = Some(params);
        let entry = ctx.emit(
            LogPayload::ParamChanged { change: *change },
            Vec::new(),
            Causes::new(ctx.trigger()),
        );
        if params.hereditary_diseases != before.hereditary_diseases {
            death::switch_diseases(domain, params.hereditary_diseases, entry, ctx);
        }
    }
}
