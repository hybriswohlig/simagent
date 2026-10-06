//! Partnerships (ADR-0004 sections 1 and 4): the fixed couples of `Start` and the input
//! `ArrangePartnership` pair a living woman and a living man.

use crate::input::{ArrangePartnership, InputRejected, RejectCode};
use crate::world::SimCtx;

use super::DomainState;

pub(crate) fn check_arrange(
    _domain: &DomainState,
    _pair: &ArrangePartnership,
) -> Result<(), InputRejected> {
    Err(InputRejected {
        code: RejectCode::NotStarted,
    })
}

pub(crate) fn apply_arrange(
    _domain: &mut DomainState,
    _pair: &ArrangePartnership,
    _ctx: &mut SimCtx<'_, '_>,
) {
}
